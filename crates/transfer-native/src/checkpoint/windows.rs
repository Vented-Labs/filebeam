//! Windows ACL operations are isolated so the checkpoint API remains safe.

use std::{
    ffi::c_void,
    fs::{self, File},
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr::{NonNull, null_mut},
};

use anyhow::{Result, bail};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree},
    Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetNamedSecurityInfoW, GetSecurityInfo, SE_FILE_OBJECT,
    },
    Security::{
        ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CreateWellKnownSid,
        DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
        GetSecurityDescriptorControl, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
        SECURITY_DESCRIPTOR_CONTROL, TOKEN_QUERY, TOKEN_USER, TokenUser,
        WinBuiltinAdministratorsSid, WinLocalSystemSid,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_FLAG_WRITE_THROUGH, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandleEx, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_ALWAYS, OPEN_EXISTING,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const INHERIT_TO_CHILDREN: u8 = 0x03;
const ACL_PROTECTED: u16 = SE_DACL_PROTECTED;
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const FILE_DELETE_CHILD: u32 = 0x0000_0040;
const GENERIC_WRITE: u32 = 0x4000_0000;
const GENERIC_ALL: u32 = 0x1000_0000;

/// Directory handles that pin a private profile and its ancestors against
/// deletion or replacement for the guard's lifetime.
pub struct PrivateProfileGuard {
    _directories: Vec<File>,
}

/// Directory handles that pin a user-approved destination and its ancestors
/// after checking its less restrictive receive-directory ACL policy.
pub struct TrustedDirectoryGuard {
    _directories: Vec<File>,
}

pub fn create_private_profile(path: &Path) -> Result<PrivateProfileGuard> {
    create_private_path(path)?;
    guard_private_profile(path)
}

pub fn guard_private_profile(path: &Path) -> Result<PrivateProfileGuard> {
    Ok(PrivateProfileGuard {
        _directories: guard_directory(path, true)?,
    })
}

pub fn guard_trusted_directory(path: &Path) -> Result<TrustedDirectoryGuard> {
    Ok(TrustedDirectoryGuard {
        _directories: guard_directory(path, false)?,
    })
}

fn guard_directory(path: &Path, private_leaf: bool) -> Result<Vec<File>> {
    let path = fs::canonicalize(path)?;
    let user = current_user_sid()?;
    let system = local_system_sid()?;
    let administrators = builtin_administrators_sid()?;
    let ancestors = path.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    let mut directories = Vec::with_capacity(ancestors.len());
    for ancestor in ancestors.into_iter().rev() {
        let directory = open_guarded_directory(&ancestor)?;
        validate_guarded_directory(
            &directory,
            &user,
            &system,
            &administrators,
            ancestor == path,
            private_leaf,
        )?;
        directories.push(directory);
    }
    Ok(directories)
}

pub(super) fn create_private_dir(path: &Path) -> io::Result<()> {
    let wide = wide_path(path)?;
    let descriptor = PrivateDescriptor::new()?;
    // This descriptor applies while the directory is created, not afterward.
    if unsafe { CreateDirectoryW(wide.as_ptr(), descriptor.attributes()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn create_private_path(path: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() {
                    bail!(
                        "private profile path is not a directory: {}",
                        current.display()
                    );
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(current.to_path_buf());
                current = current.parent().ok_or_else(|| {
                    anyhow::anyhow!("cannot create private profile: {}", path.display())
                })?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    for directory in missing.into_iter().rev() {
        create_private_dir(&directory)?;
    }
    Ok(())
}

fn open_guarded_directory(path: &Path) -> Result<File> {
    let wide = wide_path(path)?;
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_READ,
            // Deliberately omit FILE_SHARE_DELETE: holding every handle prevents
            // a later path lookup from being redirected by a renamed ancestor.
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[repr(C)]
struct FileAttributeTagInfo {
    attributes: u32,
    reparse_tag: u32,
}

fn validate_guarded_directory(
    directory: &File,
    user: &Sid,
    system: &Sid,
    administrators: &Sid,
    leaf: bool,
    private_leaf: bool,
) -> Result<()> {
    let handle = directory.as_raw_handle() as HANDLE;
    let mut tag: FileAttributeTagInfo = unsafe { zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut tag as *mut FileAttributeTagInfo).cast(),
            size_of::<FileAttributeTagInfo>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if tag.attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        bail!("private profile ancestor is not a directory");
    }
    if tag.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!("private profile path contains a reparse point");
    }

    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let result = unsafe {
        GetSecurityInfo(
            handle,
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
    let validated = unsafe {
        if leaf && private_leaf {
            verify_descriptor(
                descriptor,
                owner,
                dacl,
                user.as_psid(),
                system.as_psid(),
                true,
                true,
            )
        } else if leaf {
            verify_destination_descriptor(owner, dacl, user, system, administrators)
        } else {
            verify_ancestor_descriptor(owner, dacl, user, system, administrators)
        }
    };
    unsafe { LocalFree(descriptor.cast()) };
    validated
}

unsafe fn verify_destination_descriptor(
    owner: PSID,
    dacl: *mut ACL,
    user: &Sid,
    system: &Sid,
    administrators: &Sid,
) -> Result<()> {
    unsafe { verify_ancestor_descriptor(owner, dacl, user, system, administrators) }?;
    unsafe {
        verify_untrusted_access(dacl, user, system, administrators, untrusted_can_modify_destination)
    }
}

unsafe fn verify_ancestor_descriptor(
    owner: PSID,
    dacl: *mut ACL,
    user: &Sid,
    system: &Sid,
    administrators: &Sid,
) -> Result<()> {
    if owner.is_null()
        || dacl.is_null()
        || (unsafe { EqualSid(owner, user.as_psid()) } == 0
            && unsafe { EqualSid(owner, system.as_psid()) } == 0
            && unsafe { EqualSid(owner, administrators.as_psid()) } == 0)
    {
        bail!("private profile ancestor has an unexpected owner or null DACL");
    }
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    for index in 0..info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        unsafe {
            verify_ace_access(ace, user, system, administrators, untrusted_can_mutate_ancestor)
        }?;
    }
    Ok(())
}

unsafe fn verify_untrusted_access(
    dacl: *mut ACL,
    user: &Sid,
    system: &Sid,
    administrators: &Sid,
    prohibited: fn(u32) -> bool,
) -> Result<()> {
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    for index in 0..info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        unsafe { verify_ace_access(ace, user, system, administrators, prohibited) }?;
    }
    Ok(())
}

unsafe fn verify_ace_access(
    ace: *mut c_void,
    user: &Sid,
    system: &Sid,
    administrators: &Sid,
    prohibited: fn(u32) -> bool,
) -> Result<()> {
    let header = unsafe { &*ace.cast::<ACE_HEADER>() };
    if header.AceType != ACCESS_ALLOWED_ACE_TYPE {
        bail!("directory DACL has an unsupported ACE");
    }
    let allowed = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACCESS_ALLOWED_ACE>() };
    let sid: PSID = (&allowed.SidStart as *const u32).cast_mut().cast();
    let trusted = unsafe { EqualSid(sid, user.as_psid()) } != 0
        || unsafe { EqualSid(sid, system.as_psid()) } != 0
        || unsafe { EqualSid(sid, administrators.as_psid()) } != 0;
    if !trusted && prohibited(allowed.Mask) {
        bail!("directory grants mutable access to an untrusted principal");
    }
    Ok(())
}

fn untrusted_can_mutate_ancestor(access: u32) -> bool {
    access & (0x0001_0000 | FILE_DELETE_CHILD | WRITE_DAC | WRITE_OWNER | GENERIC_WRITE | GENERIC_ALL)
        != 0
}

fn untrusted_can_modify_destination(access: u32) -> bool {
    // FILE_ADD_FILE and FILE_ADD_SUBDIRECTORY allow content injection at the
    // approved root; DELETE prevents a held root from protecting its children.
    access & (0x0000_0002 | 0x0000_0004 | 0x0001_0000 | FILE_DELETE_CHILD | WRITE_DAC
        | WRITE_OWNER | GENERIC_WRITE | GENERIC_ALL)
        != 0
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
    well_known_sid(WinLocalSystemSid)
}

fn builtin_administrators_sid() -> io::Result<Sid> {
    well_known_sid(WinBuiltinAdministratorsSid)
}

fn well_known_sid(kind: windows_sys::Win32::Security::WELL_KNOWN_SID_TYPE) -> io::Result<Sid> {
    let mut bytes = vec![0_u8; 68]; // SECURITY_MAX_SID_SIZE
    let mut length = bytes.len() as u32;
    if unsafe { CreateWellKnownSid(kind, null_mut(), bytes.as_mut_ptr().cast(), &mut length) } == 0
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
    use super::{
        FILE_DELETE_CHILD, GENERIC_ALL, GENERIC_WRITE, WRITE_DAC, WRITE_OWNER, delete_sharing_mode,
        private_sddl, untrusted_can_modify_destination, untrusted_can_mutate_ancestor,
    };
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

    #[test]
    fn ancestor_mutation_rights_are_not_granted_to_untrusted_sids() {
        for access in [
            0x0001_0000, // DELETE
            FILE_DELETE_CHILD,
            WRITE_DAC,
            WRITE_OWNER,
            GENERIC_WRITE,
            GENERIC_ALL,
        ] {
            assert!(untrusted_can_mutate_ancestor(access));
        }
        assert!(!untrusted_can_mutate_ancestor(0x0002_0000)); // READ_CONTROL
    }

    #[test]
    fn destination_root_rejects_untrusted_content_mutation_rights() {
        for access in [
            0x0000_0002, // FILE_ADD_FILE
            0x0000_0004, // FILE_ADD_SUBDIRECTORY
            0x0001_0000, // DELETE
            FILE_DELETE_CHILD,
            WRITE_DAC,
            WRITE_OWNER,
        ] {
            assert!(untrusted_can_modify_destination(access));
        }
        assert!(!untrusted_can_modify_destination(0x0000_0020)); // FILE_TRAVERSE
    }
}
