use std::{env, io, process::Command};

pub fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("Wave Terminal: {error}");
    std::process::exit(1);
}

pub fn run(name: &str, daemon: bool) -> ! {
    let result = forward(name, daemon);
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => fail(format!("{name}: {error}")),
    }
}

fn forward(name: &str, _daemon: bool) -> io::Result<i32> {
    let exe = env::current_exe()?.canonicalize()?;
    let target = exe
        .with_file_name(format!("{name}{}", env::consts::EXE_SUFFIX))
        .canonicalize()?;
    let mut command = Command::new(&target);
    command.args(env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.arg0(&target);
        Err(command.exec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if _daemon {
            // Assign the wrapper before spawn so every descendant inherits cleanup atomically.
            daemon_job()?;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        Ok(command.status()?.code().unwrap_or(1))
    }
}

#[cfg(windows)]
fn daemon_job() -> io::Result<()> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::GetCurrentProcess,
        },
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(job, GetCurrentProcess()) == 0
        {
            let error = io::Error::last_os_error();
            CloseHandle(job);
            return Err(error);
        }
        // Keep the handle until process exit; closing it here would terminate this wrapper too.
        Ok(())
    }
}
