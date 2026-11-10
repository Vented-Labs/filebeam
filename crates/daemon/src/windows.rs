//! Windows transport and confined-root primitives.
//!
//! `lib.rs` must expose this module only on Windows and run the existing
//! nonce/HMAC protocol over `Pipe`.  The pipe ACL deliberately names the
//! current token's user SID and logon SID; it does not grant SYSTEM access.

use std::{
    ffi::c_void,
    io::{self, Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path},
    ptr::{null, null_mut},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GetLastError,
        HANDLE, INVALID_HANDLE_VALUE, LocalFree, RtlNtStatusToDosError, UNICODE_STRING,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{
        EqualSid, GetLengthSid, GetTokenInformation, RevertToSelf, TOKEN_GROUPS, TOKEN_QUERY,
        TOKEN_USER, TokenLogonSid, TokenUser,
    },
    Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_DISPOSITION_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo, FileRenameInfo, FileRenameInfoEx,
        GetFileInformationByHandleEx, GetFinalPathNameByHandleW, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
        ReadFile, SYNCHRONIZE, SetFileInformationByHandle, WriteFile,
    },
    System::{
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
            ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, WaitNamedPipeW,
        },
        Threading::{
            CreateEventW, GetCurrentProcess, GetCurrentThread, OpenProcess, OpenProcessToken,
            OpenThreadToken, PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        },
    },
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
            FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
        },
    },
    Win32::System::IO::{CancelIoEx, GetOverlappedResult, IO_STATUS_BLOCK, OVERLAPPED},
};

#[path = "platform_root.rs"]
mod platform_root;
#[path = "windows_receiver.rs"]
mod windows_receiver;

pub(crate) use platform_root::{PlatformRoot, StagedObjectIdentity};
pub(crate) use windows_receiver::{
    DaemonPeerReceiver, SharedPeerVerification, WindowsReceiverRoot,
};

const PIPE_BUFFER_BYTES: u32 = 1024 * 1024;
const PIPE_IO_TIMEOUT_MS: u32 = 5_000;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ID_INFO_CLASS: i32 = 18;
const SDDL_REVISION_1: u32 = 1;

/// A profile-specific endpoint.  Its value is not an authority: clients must
/// still authenticate the daemon with the existing nonce-bound IPC key.
pub(crate) fn pipe_name(home: &Path) -> Result<Vec<u16>> {
    let canonical = std::fs::canonicalize(home).context("canonicalize daemon profile")?;
    let mut digest = Sha256::new();
    digest.update(canonical.as_os_str().as_encoded_bytes());
    let digest = digest.finalize();
    let name = format!(r"\\.\pipe\Filebeam-daemon-{}", hex(&digest));
    Ok(wide(&name))
}

/// A blocking named-pipe endpoint.  The daemon owns the server instance; one
/// connection is handled at a time, matching the current Unix listener.
pub(crate) struct Pipe {
    handle: HANDLE,
    expected_client: Option<TokenIdentity>,
    peer_verified: bool,
}

impl Pipe {
    pub(crate) fn create_server(name: &[u16]) -> Result<Self> {
        let identity = TokenIdentity::current()?;
        let descriptor = SecurityDescriptor::for_identity(&identity)?;
        let mut attributes = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                &mut attributes,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error()).context("create daemon named pipe");
        }
        Ok(Self {
            handle,
            expected_client: Some(identity),
            peer_verified: false,
        })
    }

    pub(crate) fn connect_client(name: &[u16]) -> Result<Self> {
        if unsafe { WaitNamedPipeW(name.as_ptr(), 5_000) } == 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                return Err(crate::DaemonNotRunning {
                    endpoint: String::from_utf16_lossy(name.strip_suffix(&[0]).unwrap_or(name))
                        .into(),
                }
                .into());
            }
            return Err(error).context("wait for Filebeam daemon");
        }
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error()).context("connect to Filebeam daemon");
        }
        TokenIdentity::current()?.matches_named_pipe_server(handle)?;
        Ok(Self {
            handle,
            expected_client: None,
            peer_verified: true,
        })
    }

    pub(crate) fn accept(&self) -> Result<()> {
        connect_pipe_with_timeout(self.handle).context("accept Filebeam daemon client")
    }

    pub(crate) fn disconnect(&self) {
        unsafe { DisconnectNamedPipe(self.handle) };
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

impl Read for Pipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = pipe_read_with_timeout(self.handle, buffer)?;
        if !self.peer_verified {
            let expected = self
                .expected_client
                .as_ref()
                .expect("server pipe identity is present");
            if !expected
                .matches_impersonated_client(self.handle)
                .map_err(io::Error::other)?
            {
                self.disconnect();
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "named-pipe client is not the current user in this logon session",
                ));
            }
            self.peer_verified = true;
        }
        Ok(read as usize)
    }
}

impl Write for Pipe {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = pipe_write_with_timeout(self.handle, buffer)?;
        Ok(written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Timed overlapped writes already hand each frame to the pipe. A pipe
        // FlushFileBuffers waits for the reader and has no cancellable deadline.
        Ok(())
    }
}

fn overlapped_event() -> io::Result<(HANDLE, OVERLAPPED)> {
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    overlapped.hEvent = event;
    Ok((event, overlapped))
}

fn finish_overlapped(
    handle: HANDLE,
    event: HANDLE,
    overlapped: &mut OVERLAPPED,
) -> io::Result<u32> {
    let wait = unsafe { WaitForSingleObject(event, PIPE_IO_TIMEOUT_MS) };
    if wait == WAIT_TIMEOUT {
        unsafe { CancelIoEx(handle, overlapped) };
        let mut ignored = 0;
        unsafe { GetOverlappedResult(handle, overlapped, &mut ignored, 1) };
        unsafe { CloseHandle(event) };
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "named-pipe I/O timed out",
        ));
    }
    if wait != WAIT_OBJECT_0 {
        unsafe { CloseHandle(event) };
        return Err(io::Error::last_os_error());
    }
    let mut transferred = 0;
    let complete = unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, 0) } != 0;
    unsafe { CloseHandle(event) };
    if !complete {
        return Err(io::Error::last_os_error());
    }
    Ok(transferred)
}

fn pipe_read_with_timeout(handle: HANDLE, buffer: &mut [u8]) -> io::Result<u32> {
    let length: u32 = buffer
        .len()
        .try_into()
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    let (event, mut overlapped) = overlapped_event()?;
    let mut transferred = 0;
    let started = unsafe {
        ReadFile(
            handle,
            buffer.as_mut_ptr().cast(),
            length,
            &mut transferred,
            &mut overlapped,
        )
    } != 0;
    if started {
        unsafe { CloseHandle(event) };
        return Ok(transferred);
    }
    if unsafe { GetLastError() } != ERROR_IO_PENDING {
        unsafe { CloseHandle(event) };
        return Err(io::Error::last_os_error());
    }
    finish_overlapped(handle, event, &mut overlapped)
}

fn pipe_write_with_timeout(handle: HANDLE, buffer: &[u8]) -> io::Result<u32> {
    let length: u32 = buffer
        .len()
        .try_into()
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    let (event, mut overlapped) = overlapped_event()?;
    let mut transferred = 0;
    let started = unsafe {
        WriteFile(
            handle,
            buffer.as_ptr().cast(),
            length,
            &mut transferred,
            &mut overlapped,
        )
    } != 0;
    if started {
        unsafe { CloseHandle(event) };
        return Ok(transferred);
    }
    if unsafe { GetLastError() } != ERROR_IO_PENDING {
        unsafe { CloseHandle(event) };
        return Err(io::Error::last_os_error());
    }
    finish_overlapped(handle, event, &mut overlapped)
}

fn connect_pipe_with_timeout(handle: HANDLE) -> Result<()> {
    let (event, mut overlapped) = overlapped_event()?;
    if unsafe { ConnectNamedPipe(handle, &mut overlapped) } != 0 {
        unsafe { CloseHandle(event) };
        return Ok(());
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_PIPE_CONNECTED {
        unsafe { CloseHandle(event) };
        return Ok(());
    }
    if error != ERROR_IO_PENDING {
        unsafe { CloseHandle(event) };
        return Err(io::Error::last_os_error().into());
    }
    finish_overlapped(handle, event, &mut overlapped)
        .map(|_| ())
        .map_err(Into::into)
}

/// A granted directory handle and its stable volume/file identity.
///
/// Every descendant is opened by `NtCreateFile` with this handle as
/// `OBJECT_ATTRIBUTES::RootDirectory`. Handles deliberately omit delete
/// sharing, preventing a concurrent process from moving an ancestor while it
/// is being traversed or published.
pub(crate) struct ConfinedRoot {
    handle: std::fs::File,
    id: FileIdentity,
}

impl ConfinedRoot {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let handle = open_directory(path)?;
        let id = file_identity(handle.as_raw_handle() as HANDLE)?;
        Ok(Self { handle, id })
    }

    pub(crate) fn identity(&self) -> (u64, [u8; 16]) {
        (self.id.volume_serial, self.id.file_id)
    }

    pub(crate) fn create_new(&self, relative: &Path) -> Result<std::fs::File> {
        self.verify_identity()?;
        let components = checked_components(relative)?;
        let parent = self.open_directories(&components[..components.len() - 1])?;
        create_relative_file(&parent, components.last().expect("validated relative path"))
    }

    pub(crate) fn create_directory(&self, name: &str) -> Result<std::fs::File> {
        self.verify_identity()?;
        let name = checked_name(name)?;
        create_relative_directory(&self.handle, name)
    }

    pub(crate) fn open_directory(&self, name: &str) -> Result<std::fs::File> {
        self.verify_identity()?;
        open_relative_directory(&self.handle, checked_name(name)?)
    }

    pub(crate) fn create_directory_root(&self, name: &str) -> Result<Self> {
        let handle = self.create_directory(name)?;
        let id = file_identity(handle.as_raw_handle() as HANDLE)?;
        Ok(Self { handle, id })
    }

    pub(crate) fn open_directory_root(&self, name: &str) -> Result<Self> {
        let handle = self.open_directory(name)?;
        let id = file_identity(handle.as_raw_handle() as HANDLE)?;
        Ok(Self { handle, id })
    }

    pub(crate) fn open_or_create_subdirectory(&self, components: &[String]) -> Result<Self> {
        self.verify_identity()?;
        let handle = self.open_directories_or_create(components)?;
        let id = file_identity(handle.as_raw_handle() as HANDLE)?;
        Ok(Self { handle, id })
    }

    pub(crate) fn open_or_create_child(&self, name: &str) -> Result<Self> {
        self.open_or_create_subdirectory(&[name.into()])
    }

    pub(crate) fn tethered_path(&self) -> Result<std::path::PathBuf> {
        final_path(self.handle.as_raw_handle() as HANDLE)
    }

    pub(crate) fn try_clone(&self) -> Result<Self> {
        self.verify_identity()?;
        Ok(Self {
            handle: self.handle.try_clone()?,
            id: self.id,
        })
    }

    pub(crate) fn remove_directory(&self, name: &str) -> Result<()> {
        self.verify_identity()?;
        let directory = open_relative_directory_with_access(
            &self.handle,
            checked_name(name)?,
            FILE_GENERIC_READ | DELETE | SYNCHRONIZE,
        )?;
        mark_delete(directory.as_raw_handle() as HANDLE)
    }

    /// Publishes a held child of `staging` into this root, with no pathname
    /// lookup and no replacement. The source object's identity is checked both
    /// before and after an idempotent retry.
    pub(crate) fn rename_from_no_replace(
        &self,
        staging: &std::fs::File,
        source_name: &str,
        destination_name: &str,
        expected: FileIdentity,
    ) -> Result<()> {
        self.verify_identity()?;
        let source = open_relative_any(staging, checked_name(source_name)?, DELETE | SYNCHRONIZE)?;
        reject_reparse(source.as_raw_handle() as HANDLE)?;
        if file_identity(source.as_raw_handle() as HANDLE)? != expected {
            bail!("staged root was replaced")
        }
        rename_relative_no_replace(
            source.as_raw_handle() as HANDLE,
            self.handle.as_raw_handle() as HANDLE,
            checked_name(destination_name)?,
        )?;
        if file_identity(source.as_raw_handle() as HANDLE)? != expected {
            bail!("published destination was replaced")
        }
        Ok(())
    }

    pub(crate) fn child_identity(
        &self,
        directory: &std::fs::File,
        name: &str,
    ) -> Result<Option<FileIdentity>> {
        self.verify_identity()?;
        match open_relative_any(
            directory,
            checked_name(name)?,
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        ) {
            Ok(file) => {
                reject_reparse(file.as_raw_handle() as HANDLE)?;
                Ok(Some(file_identity(file.as_raw_handle() as HANDLE)?))
            }
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn verify_identity(&self) -> Result<()> {
        let actual = file_identity(self.handle.as_raw_handle() as HANDLE)?;
        if actual != self.id {
            bail!("destination changed since it was granted");
        }
        reject_reparse(self.handle.as_raw_handle() as HANDLE)
    }

    fn open_directories(&self, components: &[&std::ffi::OsStr]) -> Result<std::fs::File> {
        let mut directory = self.handle.try_clone()?;
        for component in components {
            directory = open_relative_directory(&directory, component)?;
        }
        Ok(directory)
    }

    fn open_directories_or_create(&self, components: &[String]) -> Result<std::fs::File> {
        let mut directory = self.handle.try_clone()?;
        for component in components {
            let component = checked_name(component)?;
            match create_relative_directory(&directory, component) {
                Ok(created) => directory = created,
                Err(error)
                    if error
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) =>
                {
                    directory = open_relative_directory(&directory, component)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(directory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    pub(crate) volume_serial: u64,
    pub(crate) file_id: [u8; 16],
}

#[repr(C)]
struct FileIdInfo {
    volume_serial: u64,
    file_id: [u8; 16],
}

#[repr(C)]
struct FileAttributeTagInfo {
    attributes: u32,
    reparse_tag: u32,
}

fn open_directory(path: &Path) -> Result<std::fs::File> {
    let path = wide_os(path.as_os_str());
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error()).context("open destination directory");
    }
    let file = unsafe { std::fs::File::from_raw_handle(handle as *mut c_void) };
    if file_attributes(handle)? & FILE_ATTRIBUTE_DIRECTORY == 0 {
        bail!("destination is not a directory");
    }
    reject_reparse(handle)?;
    Ok(file)
}

fn reject_reparse(handle: HANDLE) -> Result<()> {
    if file_attributes(handle)? & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!("destination path contains a reparse point");
    }
    Ok(())
}

fn file_attributes(handle: HANDLE) -> Result<u32> {
    let mut info: FileAttributeTagInfo = unsafe { zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut info as *mut FileAttributeTagInfo).cast(),
            size_of::<FileAttributeTagInfo>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error()).context("inspect reparse point");
    }
    Ok(info.attributes)
}

fn file_identity(handle: HANDLE) -> Result<FileIdentity> {
    let mut info: FileIdInfo = unsafe { zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FILE_ID_INFO_CLASS,
            (&mut info as *mut FileIdInfo).cast(),
            size_of::<FileIdInfo>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error()).context("read destination identity");
    }
    Ok(FileIdentity {
        volume_serial: info.volume_serial,
        file_id: info.file_id,
    })
}

fn final_path(handle: HANDLE) -> Result<std::path::PathBuf> {
    let length = unsafe { GetFinalPathNameByHandleW(handle, null_mut(), 0, 0) };
    if length == 0 {
        return Err(io::Error::last_os_error()).context("resolve confined directory handle");
    }
    let mut path = vec![0u16; length as usize + 1];
    let written =
        unsafe { GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0) };
    if written == 0 || written > length {
        return Err(io::Error::last_os_error()).context("resolve confined directory handle");
    }
    path.truncate(written as usize);
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(
        &path,
    )))
}

fn checked_components(path: &Path) -> Result<Vec<&std::ffi::OsStr>> {
    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(component) => checked_component(component),
            _ => bail!("Windows confined paths must be non-empty relative components"),
        })
        .collect::<Result<Vec<_>>>()?;
    if components.is_empty() {
        bail!("Windows confined path is empty")
    }
    Ok(components)
}

fn checked_name(component: &str) -> Result<&std::ffi::OsStr> {
    checked_component(std::ffi::OsStr::new(component))
}

pub(crate) fn validate_component_name(component: &str) -> Result<()> {
    checked_name(component).map(|_| ())
}

fn checked_component(component: &std::ffi::OsStr) -> Result<&std::ffi::OsStr> {
    let value = component
        .to_str()
        .context("Windows confined paths must be valid Unicode")?;
    let reserved = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved_device = matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || reserved
            .strip_prefix("COM")
            .or_else(|| reserved.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            });
    if value.is_empty()
        || value.len() > 255
        || value == "."
        || value == ".."
        || value.ends_with('.')
        || value.ends_with(' ')
        || reserved_device
        || value.contains([':', '\\', '/', '<', '>', '"', '|', '?', '*'])
        || value.chars().any(|character| character.is_control())
    {
        bail!("Windows confined paths require one non-ADS component")
    }
    Ok(component)
}

fn nt_error(status: i32, operation: &str) -> anyhow::Error {
    let code = unsafe { RtlNtStatusToDosError(status) };
    anyhow::anyhow!("{operation}: {}", io::Error::from_raw_os_error(code as i32))
}

fn open_relative_any(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    access: u32,
) -> Result<std::fs::File> {
    nt_open_relative(
        parent,
        name,
        access,
        FILE_OPEN,
        FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )
}

fn open_relative_directory(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
) -> Result<std::fs::File> {
    open_relative_directory_with_access(parent, name, FILE_GENERIC_READ | SYNCHRONIZE)
}

fn open_relative_directory_with_access(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    access: u32,
) -> Result<std::fs::File> {
    let file = nt_open_relative(
        parent,
        name,
        access,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )?;
    reject_reparse(file.as_raw_handle() as HANDLE)?;
    Ok(file)
}

fn create_relative_directory(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
) -> Result<std::fs::File> {
    let file = nt_open_relative(
        parent,
        name,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | SYNCHRONIZE,
        FILE_CREATE,
        FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )?;
    reject_reparse(file.as_raw_handle() as HANDLE)?;
    Ok(file)
}

fn create_relative_file(parent: &std::fs::File, name: &std::ffi::OsStr) -> Result<std::fs::File> {
    let file = nt_open_relative(
        parent,
        name,
        FILE_GENERIC_WRITE | SYNCHRONIZE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )?;
    reject_reparse(file.as_raw_handle() as HANDLE)?;
    Ok(file)
}

fn nt_open_relative(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
    access: u32,
    disposition: u32,
    options: u32,
) -> Result<std::fs::File> {
    let mut name = wide_os(name);
    name.pop(); // UNICODE_STRING is length-delimited, not NUL-terminated.
    let mut object_name = UNICODE_STRING {
        Length: (name.len() * size_of::<u16>()) as u16,
        MaximumLength: (name.len() * size_of::<u16>()) as u16,
        Buffer: name.as_mut_ptr(),
    };
    // Creation must not inherit a broad grant-directory DACL into private
    // staging. The descriptor remains alive through NtCreateFile.
    let identity = TokenIdentity::current()?;
    let descriptor = SecurityDescriptor::for_private_files(&identity)?;
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: &mut object_name,
        Attributes: windows_sys::Win32::Foundation::OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: descriptor.0.cast(),
        SecurityQualityOfService: null(),
    };
    let mut handle = null_mut();
    let mut status: IO_STATUS_BLOCK = unsafe { zeroed() };
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status,
            null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            disposition,
            options,
            null(),
            0,
        )
    };
    if result < 0 {
        return Err(nt_error(result, "open confined relative path"));
    }
    Ok(unsafe { std::fs::File::from_raw_handle(handle as *mut c_void) })
}

fn mark_delete(handle: HANDLE) -> Result<()> {
    let mut info = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfo,
            (&mut info as *mut FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error()).context("remove confined directory");
    }
    Ok(())
}

fn rename_relative_no_replace(source: HANDLE, root: HANDLE, name: &std::ffi::OsStr) -> Result<()> {
    let mut name = wide_os(name);
    name.pop();
    let name_offset = std::mem::offset_of!(
        windows_sys::Win32::Storage::FileSystem::FILE_RENAME_INFO,
        FileName
    );
    let bytes = name_offset + name.len() * size_of::<u16>();
    let words = bytes.div_ceil(size_of::<usize>());
    let mut storage = vec![0usize; words];
    let info = storage
        .as_mut_ptr()
        .cast::<windows_sys::Win32::Storage::FileSystem::FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.Flags = 0; // FileRenameInfoEx: no REPLACE_IF_EXISTS flag.
        (*info).RootDirectory = root;
        (*info).FileNameLength = (name.len() * size_of::<u16>()) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (info.cast::<u8>().add(name_offset)).cast(),
            name.len(),
        );
    }
    let rename =
        |class| unsafe { SetFileInformationByHandle(source, class, info.cast(), bytes as u32) };
    if rename(FileRenameInfoEx) != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_INVALID_PARAMETER as i32) {
        return Err(error).context("rename staged root without replacement");
    }
    if rename(FileRenameInfo) == 0 {
        return Err(io::Error::last_os_error()).context("rename staged root without replacement");
    }
    Ok(())
}

struct TokenIdentity {
    user: Vec<u8>,
    logon: Vec<u8>,
}

impl TokenIdentity {
    fn current() -> Result<Self> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error()).context("open current process token");
        }
        let result = Self::from_token(token);
        unsafe { CloseHandle(token) };
        result
    }

    fn matches_impersonated_client(&self, pipe: HANDLE) -> Result<bool> {
        if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
            return Err(io::Error::last_os_error()).context("impersonate named-pipe client");
        }
        let result = (|| {
            let mut token = null_mut();
            if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } == 0 {
                return Err(io::Error::last_os_error()).context("open named-pipe client token");
            }
            let identity = Self::from_token(token);
            unsafe { CloseHandle(token) };
            Ok(identity.map(|identity| self.same_as(&identity))?)
        })();
        unsafe { RevertToSelf() };
        result
    }

    fn matches_named_pipe_server(&self, pipe: HANDLE) -> Result<()> {
        let mut process_id = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe, &mut process_id) } == 0 {
            return Err(io::Error::last_os_error()).context("read named-pipe server process ID");
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return Err(io::Error::last_os_error()).context("open named-pipe server process");
        }
        let result = (|| {
            let mut token = null_mut();
            if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
                return Err(io::Error::last_os_error()).context("open named-pipe server token");
            }
            let identity = Self::from_token(token);
            unsafe { CloseHandle(token) };
            if !self.same_as(&identity?) {
                bail!("named-pipe server is not the current user in this logon session")
            }
            Ok(())
        })();
        unsafe { CloseHandle(process) };
        result
    }

    fn from_token(token: HANDLE) -> Result<Self> {
        Ok(Self {
            user: token_sid(token, TokenUser)?,
            logon: token_sid(token, TokenLogonSid)?,
        })
    }

    fn same_as(&self, other: &Self) -> bool {
        (unsafe {
            EqualSid(
                self.user.as_ptr() as *mut c_void,
                other.user.as_ptr() as *mut c_void,
            ) != 0
        }) && (unsafe {
            EqualSid(
                self.logon.as_ptr() as *mut c_void,
                other.logon.as_ptr() as *mut c_void,
            ) != 0
        })
    }
}

fn token_sid(token: HANDLE, class: i32) -> Result<Vec<u8>> {
    let mut length = 0;
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut length) };
    let mut bytes = vec![0u8; length as usize];
    if unsafe { GetTokenInformation(token, class, bytes.as_mut_ptr().cast(), length, &mut length) }
        == 0
    {
        return Err(io::Error::last_os_error()).context("read token SID");
    }
    let sid = if class == TokenUser {
        unsafe { (*(bytes.as_ptr() as *const TOKEN_USER)).User.Sid }
    } else {
        unsafe { (*(bytes.as_ptr() as *const TOKEN_GROUPS)).Groups[0].Sid }
    };
    let sid_length = unsafe { GetLengthSid(sid) } as usize;
    if sid.is_null() || sid_length == 0 {
        bail!("token does not contain a required SID");
    }
    Ok(unsafe { std::slice::from_raw_parts(sid.cast(), sid_length) }.to_vec())
}

struct SecurityDescriptor(*mut c_void);

impl SecurityDescriptor {
    fn for_identity(identity: &TokenIdentity) -> Result<Self> {
        let user = sid_string(&identity.user)?;
        let logon = sid_string(&identity.logon)?;
        // GR/GW are intentionally the only pipe rights granted to either SID.
        let sddl = wide(&format!("D:(A;;GRGW;;;{user})(A;;GRGW;;;{logon})"));
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error()).context("build named-pipe DACL");
        }
        Ok(Self(descriptor))
    }

    fn for_private_files(identity: &TokenIdentity) -> Result<Self> {
        let user = sid_string(&identity.user)?;
        let logon = sid_string(&identity.logon)?;
        // Protected, inheritable ACEs keep private staging out of a broad
        // destination DACL while allowing the current logon to traverse it.
        let sddl = wide(&format!("D:P(A;OICI;GA;;;{user})(A;OICI;GA;;;{logon})"));
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error()).context("build private staging DACL");
        }
        Ok(Self(descriptor))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

fn sid_string(sid: &[u8]) -> Result<String> {
    let mut value = null_mut();
    if unsafe { ConvertSidToStringSidW(sid.as_ptr() as *mut c_void, &mut value) } == 0 {
        return Err(io::Error::last_os_error()).context("format token SID");
    }
    let length = unsafe { (0..).take_while(|index| *value.add(*index) != 0).count() };
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(value, length) })
        .context("token SID is invalid UTF-16");
    unsafe { LocalFree(value.cast()) };
    result
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn wide_os(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_pipe_accepts_the_current_logon_session() {
        let name = wide(&format!(r"\\.\pipe\Filebeam-test-{}", uuid::Uuid::new_v4()));
        let server_name = name.clone();
        let (ready, wait_for_server) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let mut server = Pipe::create_server(&server_name).unwrap();
            ready.send(()).unwrap();
            server.accept().unwrap();
            let mut byte = [0];
            server.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [7]);
        });
        wait_for_server.recv().unwrap();
        let mut client = Pipe::connect_client(&name).unwrap();
        client.write_all(&[7]).unwrap();
        thread.join().unwrap();
    }

    #[test]
    fn confined_root_never_replaces_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let root = ConfinedRoot::open(directory.path()).unwrap();
        root.create_new(Path::new("new-file")).unwrap();
        assert!(root.create_new(Path::new("new-file")).is_err());
        assert!(root.create_new(Path::new("nested/file")).is_err());
    }

    #[test]
    #[ignore = "NOT_RUN: requires a Windows NTFS test host"]
    fn confined_root_stages_nested_trees_and_publishes_without_replacing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = ConfinedRoot::open(directory.path())?;
        let stage = root.open_or_create_subdirectory(&["stage".into()])?;
        stage.create_directory("bundle")?;
        stage.create_new(Path::new("bundle/nested/file.txt"))?;
        let identity = root
            .child_identity(&stage.handle, "bundle")?
            .expect("staged root exists");
        root.rename_from_no_replace(&stage.handle, "bundle", "published", identity)?;
        assert!(root.create_new(Path::new("published/another.txt")).is_ok());
        assert!(
            root.rename_from_no_replace(&stage.handle, "bundle", "published", identity)
                .is_err()
        );
        Ok(())
    }
}
