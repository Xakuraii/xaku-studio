#[cfg(windows)]
mod imp {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    pub struct WinJobObject {
        handle: HANDLE,
    }

    unsafe impl Send for WinJobObject {}
    unsafe impl Sync for WinJobObject {}

    impl WinJobObject {
        pub fn new() -> Result<Self, String> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(format!(
                        "CreateJobObjectW failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    let err = std::io::Error::last_os_error();
                    CloseHandle(handle);
                    return Err(format!("SetInformationJobObject failed: {err}"));
                }

                Ok(Self { handle })
            }
        }

        pub fn assign_pid(&self, pid: u32) -> Result<(), String> {
            unsafe {
                let proc_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if proc_handle.is_null() {
                    return Err(format!(
                        "OpenProcess({pid}) failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                let ok = AssignProcessToJobObject(self.handle, proc_handle);
                let err = std::io::Error::last_os_error();
                CloseHandle(proc_handle);
                if ok == 0 {
                    return Err(format!("AssignProcessToJobObject({pid}) failed: {err}"));
                }
                Ok(())
            }
        }
    }

    impl Drop for WinJobObject {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null() {
                    CloseHandle(self.handle);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct WinJobObject;
    impl WinJobObject {
        pub fn new() -> Result<Self, String> {
            Ok(Self)
        }
        pub fn assign_pid(&self, _pid: u32) -> Result<(), String> {
            Ok(())
        }
    }
}

pub use imp::WinJobObject;
