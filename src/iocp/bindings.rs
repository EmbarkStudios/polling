// #[minwin]
// mod boop {

// }

mod bindings {
    #![allow(non_camel_case_types, non_snake_case)]

    pub type WIN32_ERROR = u32;
    pub const ERROR_INVALID_HANDLE: WIN32_ERROR = 6;
    pub const ERROR_IO_PENDING: WIN32_ERROR = 997;

    pub type NTSTATUS = i32;
    pub const STATUS_SUCCESS: NTSTATUS = 0;
    pub const STATUS_PENDING: NTSTATUS = 259;
    pub const STATUS_NOT_FOUND: NTSTATUS = -1073741275;
    pub const STATUS_CANCELLED: NTSTATUS = -1073741536;

    pub const SOCKET_ERROR: i32 = -1;

    pub const SIO_BASE_HANDLE: u32 = 1207959586;
    pub const SIO_BSP_HANDLE_POLL: u32 = 1207959581;

    pub type BOOL = i32;
    pub type HANDLE = isize;
    pub const INVALID_HANDLE_VALUE: HANDLE = -1;
    pub type SOCKET = usize;

    #[repr(C)]
    pub struct OVERLAPPED_0_0 {
        pub Offset: u32,
        pub OffsetHigh: u32,
    }
    #[repr(C)]
    pub union OVERLAPPED_0 {
        pub Anonymous: std::mem::ManuallyDrop<OVERLAPPED_0_0>,
        pub Pointer: *mut std::ffi::c_void,
    }
    #[repr(C)]
    pub struct OVERLAPPED {
        pub Internal: usize,
        pub InternalHigh: usize,
        pub Anonymous: OVERLAPPED_0,
        pub hEvent: HANDLE,
    }
    pub type LPWSAOVERLAPPED_COMPLETION_ROUTINE = Option<
        unsafe extern "system" fn(
            dwError: u32,
            cbTransferred: u32,
            lpOverlapped: *mut OVERLAPPED,
            dwFlags: u32,
        ),
    >;

    pub type HMODULE = isize;
    pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;

    pub const INFINITE: u32 = 4294967295;
    pub const FILE_SKIP_SET_EVENT_ON_HANDLE: u32 = 2;

    #[repr(C)]
    pub union IO_STATUS_BLOCK_0 {
        pub Status: NTSTATUS,
        pub Pointer: *mut std::ffi::c_void,
    }
    #[repr(C)]
    pub struct IO_STATUS_BLOCK {
        pub Anonymous: IO_STATUS_BLOCK_0,
        pub Information: usize,
    }
    #[repr(C)]
    pub struct UNICODE_STRING {
        pub Length: u16,
        pub MaximumLength: u16,
        pub Buffer: *mut u16,
    }
    #[repr(C)]
    pub struct OBJECT_ATTRIBUTES {
        pub Length: u32,
        pub RootDirectory: HANDLE,
        pub ObjectName: *mut UNICODE_STRING,
        pub Attributes: u32,
        pub SecurityDescriptor: *mut std::ffi::c_void,
        pub SecurityQualityOfService: *mut std::ffi::c_void,
    }

    #[repr(C)]
    pub struct OVERLAPPED_ENTRY {
        pub lpCompletionKey: usize,
        pub lpOverlapped: *mut OVERLAPPED,
        pub Internal: usize,
        pub dwNumberOfBytesTransferred: u32,
    }

    pub type NTCREATEFILE_CREATE_DISPOSITION = u32;
    pub const FILE_OPEN: NTCREATEFILE_CREATE_DISPOSITION = 1;

    pub type FILE_SHARE_MODE = u32;
    pub const FILE_SHARE_READ: FILE_SHARE_MODE = 1;
    pub const FILE_SHARE_WRITE: FILE_SHARE_MODE = 2;

    pub const SYNCHRONIZE: u32 = 1048576;

    #[link(name = "kernel32", kind = "raw-dylib")]
    extern "system" {
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
        pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> FARPROC;
        pub fn CreateIoCompletionPort(
            FileHandle: HANDLE,
            ExistingCompletionPort: HANDLE,
            CompletionKey: usize,
            NumberOfConcurrentThreads: u32,
        ) -> HANDLE;
        pub fn GetQueuedCompletionStatusEx(
            CompletionPort: HANDLE,
            lpCompletionPortEntries: *mut OVERLAPPED_ENTRY,
            ulCount: u32,
            ulNumEntriesRemoved: *mut u32,
            dwMilliseconds: u32,
            fAlertable: BOOL,
        ) -> BOOL;
        pub fn PostQueuedCompletionStatus(
            CompletionPort: HANDLE,
            dwNumberOfBytesTransferred: u32,
            dwCompletionKey: usize,
            lpOverlapped: *const OVERLAPPED,
        ) -> BOOL;
        pub fn SetFileCompletionNotificationModes(FileHandle: HANDLE, Flags: u8) -> BOOL;
    }
    #[link(name = "ws2_32", kind = "raw-dylib")]
    extern "system" {
        pub fn WSAIoctl(
            s: SOCKET,
            dwIoControlCode: u32,
            lpvInBuffer: *const std::ffi::c_void,
            cbInBuffer: u32,
            lpvOutBuffer: *mut std::ffi::c_void,
            cbOutBuffer: u32,
            lpcbBytesReturned: *mut u32,
            lpOverlapped: *mut OVERLAPPED,
            lpCompletionRoutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        ) -> i32;
    }
}
pub(crate) use bindings::*;
