//! Windows ACL operations are isolated so the checkpoint API remains safe.

use std::{
    ffi::c_void,
    fs::File,
    io,
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::Path,
    ptr::{NonNull, null_mut},
};

use anyhow::{Result, bail};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree},
    Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetNamedSecurityInfoW, SE_FILE_OBJECT,
    },
    Security::{
        ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CreateWellKnownSid,
        DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
        GetSecurityDescriptorControl, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
        SECURITY_DESCRIPTOR_CONTROL, TOKEN_QUERY, TOKEN_USER, TokenUser, WinLocalSystemSid,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL,
        FILE_FLAG_WRITE_THROUGH, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        MoveFileExW, OPEN_ALWAYS,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const INHERIT_TO_CHILDREN: u8 = 0x03;
const ACL_PROTECTED: u16 = SE_DACL_PROTECTED;
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;

pub(super) fn create_private_dir(path: &Path) -> io::Result<()> {
    let wide = wide_path(path)?;
    let descriptor = PrivateDescriptor::new()?;
    // This descriptor applies while the directory is created, not afterward.
    if unsafe { CreateDirectoryW(wide.as_ptr(), descriptor.attributes()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn open_private_file(path: &Path, readable: bool, create_new: bool) -> io::Result<File> {
    let wide = wide_path(path)?;
    let descriptor = PrivateDescriptor::new()?;
    let access = FILE_GENERIC_WRITE | if readable { FILE_GENERIC_READ } else { 0 };
    let disposition = if create_new { CREATE_NEW } else { OPEN_ALWAYS };
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            delete_sharing_mode(),
            descriptor.attributes(),
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_WRITE_THROUGH,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub(super) fn validate_private_path(path: &Path, directory: bool, strict: bool) -> Result<()> {
    let wide = wide_path(path)?;
    let user = current_user_sid()?;
    let system = local_system_sid()?;
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let result = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    let verified = unsafe {
        verify_descriptor(
            descriptor,
            owner,
            dacl,
            user.as_psid(),
            system.as_psid(),
            directory,
            strict,
        )
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    verified
}

pub(super) fn replace_file(
    source: &Path,
    destination: &Path,
    replace_existing: bool,
) -> io::Result<()> {
    let source = wide_path(source)?;
    let destination = wide_path(destination)?;
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace_existing {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct Sid {
    bytes: Vec<u8>,
}

impl Sid {
    fn as_psid(&self) -> PSID {
        self.bytes.as_ptr().cast_mut().cast()
    }
}

struct PrivateDescriptor {
    descriptor: NonNull<c_void>,
    attributes: SECURITY_ATTRIBUTES,
}

impl PrivateDescriptor {
    fn new() -> io::Result<Self> {
        let user = current_user_sid()?;
        let user_string = sid_string(user.as_psid())?;
        let sddl = private_sddl(&user_string);
        let mut wide: Vec<u16> = sddl.encode_utf16().collect();
        wide.push(0);
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SECURITY_DESCRIPTOR_REVISION,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let descriptor = NonNull::new(descriptor).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Windows returned null descriptor")
        })?;
        Ok(Self {
            attributes: SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.as_ptr().cast(),
                bInheritHandle: 0,
            },
            descriptor,
        })
    }

    fn attributes(&self) -> *const SECURITY_ATTRIBUTES {
        &self.attributes
    }
}

fn private_sddl(owner_sid: &str) -> String {
    // P disables inherited ACEs; OI/CI carries these same two ACEs to children.
    format!("O:{owner_sid}D:P(A;OICI;FA;;;{owner_sid})(A;OICI;FA;;;SY)")
}

fn delete_sharing_mode() -> u32 {
    // A protocol discard releases Store's advisory lock before renaming the job
    // directory to its private tombstone. FILE_SHARE_DELETE keeps that rename
    // compatible with any still-closing checkpoint or lock file handle.
    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
}

impl Drop for PrivateDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor.as_ptr().cast());
        }
    }
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    wide.push(0);
    Ok(wide)
}

fn current_user_sid() -> io::Result<Sid> {
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut bytes = 0;
        unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0_u8; bytes as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        let sid_length = unsafe { GetLengthSid(user.User.Sid) } as usize;
        let bytes =
            unsafe { std::slice::from_raw_parts(user.User.Sid.cast::<u8>(), sid_length) }.to_vec();
        Ok(Sid { bytes })
    })();
    unsafe {
        CloseHandle(token);
    }
    result
}

fn local_system_sid() -> io::Result<Sid> {
    let mut bytes = vec![0_u8; 68]; // SECURITY_MAX_SID_SIZE
    let mut length = bytes.len() as u32;
    if unsafe {
        CreateWellKnownSid(
            WinLocalSystemSid,
            null_mut(),
            bytes.as_mut_ptr().cast(),
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(length as usize);
    Ok(Sid { bytes })
}

fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid token SID"));
    unsafe {
        LocalFree(text.cast());
    }
    result
}

unsafe fn verify_descriptor(
    descriptor: PSECURITY_DESCRIPTOR,
    owner: PSID,
    dacl: *mut ACL,
    user: PSID,
    system: PSID,
    directory: bool,
    strict: bool,
) -> Result<()> {
    if descriptor.is_null() || dacl.is_null() || unsafe { EqualSid(owner, user) } == 0 {
        bail!("private path is not owned by the current Windows user");
    }
    let mut control: SECURITY_DESCRIPTOR_CONTROL = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & ACL_PROTECTED == 0
    {
        bail!("private path does not have a protected DACL");
    }
    if !strict {
        return Ok(());
    }
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast::<c_void>(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if info.AceCount != 2 {
        bail!("private path DACL must contain only owner and SYSTEM ACEs");
    }
    let mut owner_seen = false;
    let mut system_seen = false;
    for index in 0..info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE
            || header.AceFlags & INHERIT_TO_CHILDREN != INHERIT_TO_CHILDREN
        {
            bail!("private path DACL has an unexpected ACE");
        }
        let allowed = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACCESS_ALLOWED_ACE>() };
        let ace_sid: PSID = (&allowed.SidStart as *const u32).cast_mut().cast();
        if allowed.Mask != FILE_ALL_ACCESS {
            bail!("private path DACL does not grant full access");
        }
        if unsafe { EqualSid(ace_sid, user) } != 0 {
            owner_seen = true;
        } else if unsafe { EqualSid(ace_sid, system) } != 0 {
            system_seen = true;
        } else {
            bail!("private path DACL grants access to an unexpected principal");
        }
    }
    if !owner_seen || !system_seen {
        bail!("private path DACL is missing owner or SYSTEM access");
    }
    let _ = directory; // Both file and directory descriptors explicitly retain OI/CI.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{delete_sharing_mode, private_sddl};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    #[test]
    fn protected_dacl_sddl_grants_only_owner_and_system() {
        assert_eq!(
            private_sddl("S-1-5-21-100-200-300-400"),
            "O:S-1-5-21-100-200-300-400D:P(A;OICI;FA;;;S-1-5-21-100-200-300-400)(A;OICI;FA;;;SY)"
        );
    }

    #[test]
    fn open_handles_allow_private_tombstone_rename() {
        assert_eq!(
            delete_sharing_mode(),
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        );
    }
}
