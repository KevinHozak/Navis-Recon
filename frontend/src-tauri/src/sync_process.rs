//! Bounded process runner for native analyzer subprocesses.
//!
//! Windows uses a private Job Object so cancellation and timeout can never
//! affect processes outside this invocation. Other platforms fail closed.
use std::time::Duration;

pub const DEFAULT_TIMEOUT_SECONDS: u64 = 600;
pub const MAX_TIMEOUT_SECONDS: u64 = 3600;
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RETAINED_LOGS: usize = 1000;
#[cfg(all(test, windows))]
static TEST_RESUME_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(all(test, windows))]
static TEST_RESUME_UNCERTAIN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn timeout_seconds(value: &str) -> Result<u64, String> {
    let seconds: u64 = value
        .parse()
        .map_err(|_| "sync_analysis_timeout_seconds must be an integer from 1 to 3600")?;
    if !(1..=MAX_TIMEOUT_SECONDS).contains(&seconds) {
        return Err("sync_analysis_timeout_seconds must be an integer from 1 to 3600".into());
    }
    Ok(seconds)
}

/// Encodes one argv element using the Microsoft C runtime command-line rules.
pub fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') {
        return arg.to_owned();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in arg.chars() {
        if ch == '\\' {
            slashes += 1;
        } else if ch == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
            out.push('"');
            slashes = 0;
        } else {
            out.push_str(&"\\".repeat(slashes));
            slashes = 0;
            out.push(ch);
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[derive(Debug)]
pub struct RunReport {
    pub exit_code: Option<u32>,
    pub ended: bool,
    pub cancelled: bool,
    pub timed_out: bool,
    pub output_limited: bool,
    pub error: Option<String>,
}

#[cfg(not(windows))]
pub fn run<F, C>(
    _program: &str,
    _args: &[String],
    _timeout: Duration,
    _cancelled: C,
    _line: F,
) -> RunReport
where
    F: FnMut(bool, &str),
    C: Fn() -> bool,
{
    RunReport {
        exit_code: None,
        ended: true,
        cancelled: false,
        timed_out: false,
        output_limited: false,
        error: Some("Native bounded analyzer runner is supported only on Windows".into()),
    }
}

#[cfg(windows)]
mod windows_runner {
    use super::*;
    use std::{ptr, time::Instant};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
        },
        Security::SECURITY_ATTRIBUTES,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
                SetInformationJobObject, TerminateJobObject,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Pipes::{CreatePipe, PeekNamedPipe},
            Threading::{
                CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
                ResumeThread, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
                CREATE_NO_WINDOW, CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT,
                PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESHOWWINDOW,
                STARTF_USESTDHANDLES, STARTUPINFOEXW,
            },
        },
    };

    struct Owned(HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }
    fn pipe() -> Result<(Owned, Owned), String> {
        let mut read = ptr::null_mut();
        let mut write = ptr::null_mut();
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: ptr::null_mut(),
            bInheritHandle: 1,
        };
        if unsafe { CreatePipe(&mut read, &mut write, &mut sa, 0) } == 0 {
            return Err("CreatePipe failed".into());
        }
        if unsafe { SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0) } == 0 {
            unsafe {
                CloseHandle(read);
                CloseHandle(write);
            }
            return Err("SetHandleInformation failed".into());
        }
        Ok((Owned(read), Owned(write)))
    }
    pub fn run<F, C>(
        program: &str,
        args: &[String],
        timeout: Duration,
        cancelled: C,
        mut on_line: F,
    ) -> RunReport
    where
        F: FnMut(bool, &str),
        C: Fn() -> bool,
    {
        let result = unsafe { run_inner(program, args, timeout, cancelled, &mut on_line) };
        result.unwrap_or_else(|error| {
            let process_may_exist = error != "CreateProcessW failed"
                && !error.contains("CreatePipe")
                && !error.contains("CreateJobObjectW")
                && !error.contains("SetInformationJobObject")
                && !error.contains("InitializeProcThreadAttributeList");
            RunReport {
                exit_code: None,
                ended: !process_may_exist,
                cancelled: false,
                timed_out: false,
                output_limited: false,
                error: Some(error),
            }
        })
    }

    unsafe fn run_inner<F, C>(
        program: &str,
        args: &[String],
        timeout: Duration,
        cancelled: C,
        on_line: &mut F,
    ) -> Result<RunReport, String>
    where
        F: FnMut(bool, &str),
        C: Fn() -> bool,
    {
        let job = Owned(CreateJobObjectW(ptr::null(), ptr::null()));
        if job.0.is_null() {
            return Err("CreateJobObjectW failed".into());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            return Err("SetInformationJobObject failed".into());
        }
        let (out_read, out_write) = pipe()?;
        let (err_read, err_write) = pipe()?;
        let mut si: STARTUPINFOEXW = std::mem::zeroed();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;
        si.StartupInfo.wShowWindow = 0;
        si.StartupInfo.hStdOutput = out_write.0;
        si.StartupInfo.hStdError = err_write.0;
        let mut attr_size = 0usize;
        let _ = InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut attr_size);
        let mut attr_storage = vec![0usize; attr_size.div_ceil(std::mem::size_of::<usize>())];
        let attr_list = attr_storage.as_mut_ptr().cast();
        if InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size) == 0 {
            return Err("InitializeProcThreadAttributeList failed".into());
        }
        let inherited = [out_write.0, err_write.0];
        if UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited.as_ptr().cast(),
            std::mem::size_of_val(&inherited),
            ptr::null_mut(),
            ptr::null(),
        ) == 0
        {
            DeleteProcThreadAttributeList(attr_list);
            return Err("UpdateProcThreadAttribute failed".into());
        }
        si.lpAttributeList = attr_list;
        let mut command = quote_windows_arg(program);
        for arg in args {
            command.push(' ');
            command.push_str(&quote_windows_arg(arg));
        }
        let mut cmd: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        if CreateProcessW(
            ptr::null(),
            cmd.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
            ptr::null(),
            ptr::null(),
            &si.StartupInfo,
            &mut pi,
        ) == 0
        {
            DeleteProcThreadAttributeList(attr_list);
            return Err("CreateProcessW failed".into());
        }
        DeleteProcThreadAttributeList(attr_list);
        let process = Owned(pi.hProcess);
        let thread = Owned(pi.hThread);
        drop(out_write);
        drop(err_write);
        if AssignProcessToJobObject(job.0, process.0) == 0 {
            let requested = TerminateProcess(process.0, 1) != 0;
            let ended = WaitForSingleObject(process.0, 2000) == WAIT_OBJECT_0;
            return Ok(RunReport {
                exit_code: None,
                ended,
                cancelled: false,
                timed_out: false,
                output_limited: false,
                error: Some(if requested && ended {
                    "AssignProcessToJobObject failed; suspended process was terminated".into()
                } else {
                    "AssignProcessToJobObject failed and process termination is uncertain".into()
                }),
            });
        }
        if cancelled() {
            let ok = TerminateJobObject(job.0, 1) != 0;
            let ended = WaitForSingleObject(process.0, 2000) == WAIT_OBJECT_0;
            return Ok(RunReport {
                exit_code: None,
                ended,
                cancelled: true,
                timed_out: false,
                output_limited: false,
                error: (!ok || !ended).then(|| {
                    "Cancellation before resume could not confirm process termination".into()
                }),
            });
        }
        #[cfg(test)]
        let resume_failed = TEST_RESUME_FAILURE.swap(false, std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(test))]
        let resume_failed = false;
        if resume_failed || ResumeThread(thread.0) == u32::MAX {
            let requested = TerminateJobObject(job.0, 1) != 0;
            let until = Instant::now() + Duration::from_secs(2);
            let mut ended = false;
            while Instant::now() < until {
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                if QueryInformationJobObject(
                    job.0,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as _,
                    std::mem::size_of_val(&accounting) as u32,
                    ptr::null_mut(),
                ) != 0
                    && accounting.ActiveProcesses == 0
                {
                    ended = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            #[cfg(test)]
            let ended =
                ended && !TEST_RESUME_UNCERTAIN.swap(false, std::sync::atomic::Ordering::SeqCst);
            return Ok(RunReport {
                exit_code: None,
                ended,
                cancelled: false,
                timed_out: false,
                output_limited: false,
                error: Some(if requested && ended {
                    "ResumeThread failed; owned process tree was terminated".into()
                } else {
                    "ResumeThread failed and owned process termination is uncertain".into()
                }),
            });
        }
        drop(thread);
        let started = Instant::now();
        let mut total = 0usize;
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        let mut out_open = true;
        let mut err_open = true;
        let mut code = None;
        let mut root_exited_at = None;
        let mut terminating = None;
        let mut reason_cancel = false;
        let mut reason_timeout = false;
        let mut limited = false;
        let mut invalid_utf8 = false;
        while out_open || err_open || code.is_none() {
            if terminating.is_none() {
                reason_cancel = cancelled();
                reason_timeout = started.elapsed() >= timeout;
                if reason_cancel || reason_timeout {
                    let _ = TerminateJobObject(job.0, 2);
                    terminating = Some(Instant::now());
                }
            }
            for (handle, buffer, is_err, open) in [
                (out_read.0, &mut out_buf, false, &mut out_open),
                (err_read.0, &mut err_buf, true, &mut err_open),
            ] {
                if !*open {
                    continue;
                }
                let mut available = 0u32;
                if PeekNamedPipe(
                    handle,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    &mut available,
                    ptr::null_mut(),
                ) == 0
                {
                    *open = false;
                    continue;
                }
                if available > 0 {
                    let remaining_total = MAX_OUTPUT_BYTES.saturating_sub(total);
                    let remaining_line = MAX_LINE_BYTES
                        .saturating_add(1)
                        .saturating_sub(buffer.len());
                    let take = (available as usize)
                        .min(8192)
                        .min(remaining_total)
                        .min(remaining_line);
                    if take == 0 {
                        limited = true;
                        let _ = TerminateJobObject(job.0, 3);
                        terminating = Some(Instant::now());
                        continue;
                    }
                    let mut bytes = vec![0u8; take];
                    let mut got = 0u32;
                    let ok = windows_sys::Win32::Storage::FileSystem::ReadFile(
                        handle,
                        bytes.as_mut_ptr() as _,
                        take as u32,
                        &mut got,
                        ptr::null_mut(),
                    );
                    if ok == 0 {
                        *open = false;
                        continue;
                    }
                    bytes.truncate(got as usize);
                    total += bytes.len();
                    if total >= MAX_OUTPUT_BYTES && available > got {
                        limited = true;
                        let _ = TerminateJobObject(job.0, 3);
                        terminating = Some(Instant::now());
                    }
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                        let mut line = buffer.drain(..=pos).collect::<Vec<_>>();
                        line.pop();
                        if line.last() == Some(&b'\r') {
                            line.pop();
                        }
                        if line.len() > MAX_LINE_BYTES {
                            limited = true;
                            let _ = TerminateJobObject(job.0, 3);
                            terminating = Some(Instant::now());
                            break;
                        }
                        match std::str::from_utf8(&line) {
                            Ok(text) => on_line(is_err, text),
                            Err(_) => {
                                invalid_utf8 = true;
                                let _ = TerminateJobObject(job.0, 6);
                                terminating = Some(Instant::now());
                                buffer.clear();
                                break;
                            }
                        }
                    }
                    if buffer.len() > MAX_LINE_BYTES {
                        limited = true;
                        let _ = TerminateJobObject(job.0, 3);
                        terminating = Some(Instant::now());
                    }
                }
            }
            if code.is_none() && WaitForSingleObject(process.0, 0) == WAIT_OBJECT_0 {
                let mut value = 0;
                if windows_sys::Win32::System::Threading::GetExitCodeProcess(process.0, &mut value)
                    != 0
                {
                    code = Some(value);
                    root_exited_at = Some(Instant::now());
                }
            }
            if code.is_some()
                && (out_open || err_open)
                && terminating.is_none()
                && root_exited_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(2))
            {
                let _ = TerminateJobObject(job.0, 4);
                terminating = Some(Instant::now());
            }
            if terminating.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let root_ended = WaitForSingleObject(process.0, 2000) == WAIT_OBJECT_0;
        let _ = TerminateJobObject(job.0, 5);
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
        let all_ended = QueryInformationJobObject(
            job.0,
            JobObjectBasicAccountingInformation,
            &mut accounting as *mut _ as _,
            std::mem::size_of_val(&accounting) as u32,
            ptr::null_mut(),
        ) != 0
            && accounting.ActiveProcesses == 0;
        let ended = root_ended && all_ended;
        let exit_code = if ended {
            let mut value = 0;
            (GetExitCodeProcess(process.0, &mut value) != 0).then_some(value)
        } else {
            None
        };
        for (buffer, is_err) in [(&mut out_buf, false), (&mut err_buf, true)] {
            if !buffer.is_empty() && buffer.len() <= MAX_LINE_BYTES {
                if let Ok(text) = std::str::from_utf8(buffer) {
                    on_line(is_err, text);
                }
            }
        }
        Ok(RunReport {
            exit_code,
            ended,
            cancelled: reason_cancel,
            timed_out: reason_timeout,
            output_limited: limited,
            error: if !ended {
                Some("Unable to confirm analyzer process termination".into())
            } else if invalid_utf8 {
                Some("Analyzer output contains invalid UTF-8".into())
            } else {
                None
            },
        })
    }
    use windows_sys::Win32::System::Threading::GetExitCodeProcess;
}

#[cfg(windows)]
pub use windows_runner::run;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_quoting_handles_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg("two words"), "\"two words\"");
        assert_eq!(quote_windows_arg("a\\\"b"), "\"a\\\\\\\"b\"");
        assert_eq!(quote_windows_arg("tail "), "\"tail \"");
    }
    #[test]
    fn timeout_configuration_is_bounded() {
        assert_eq!(timeout_seconds("600"), Ok(600));
        assert!(timeout_seconds("0").is_err());
        assert!(timeout_seconds("3601").is_err());
        assert!(timeout_seconds("wat").is_err());
    }

    #[cfg(windows)]
    fn script(body: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "navis-sync-process-{}-{}.py",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[cfg(windows)]
    fn run_script(body: &str, timeout: Duration) -> RunReport {
        let path = script(body);
        let report = super::run(
            "python",
            &[path.to_string_lossy().into_owned()],
            timeout,
            || false,
            |_, _| {},
        );
        let _ = std::fs::remove_file(path);
        report
    }

    #[cfg(windows)]
    #[test]
    fn runner_bounds_timeout_and_kills_descendants_holding_stdout() {
        let timed = run_script("import time\ntime.sleep(30)\n", Duration::from_millis(250));
        assert!(timed.timed_out);
        assert!(timed.ended);

        let descendant = run_script(
            "import subprocess,sys\nsubprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\n",
            Duration::from_secs(8),
        );
        assert!(descendant.ended);
        assert!(descendant.error.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn runner_cancels_only_owned_job_and_blocks_prelaunch_resume() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let script_path = script("import time\ntime.sleep(30)\n");
        let mut independent = std::process::Command::new("python")
            .args(["-c", "import time; time.sleep(5)"])
            .spawn()
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = cancel.clone();
        let path = script_path.to_string_lossy().into_owned();
        let worker = std::thread::spawn(move || {
            super::run(
                "python",
                &[path],
                Duration::from_secs(15),
                || cancel_worker.load(Ordering::SeqCst),
                |_, _| {},
            )
        });
        std::thread::sleep(Duration::from_millis(300));
        cancel.store(true, Ordering::SeqCst);
        let cancelled = worker.join().unwrap();
        assert!(independent.try_wait().unwrap().is_none());
        let _ = independent.kill();
        let _ = independent.wait();
        let _ = std::fs::remove_file(script_path);
        assert!(cancelled.cancelled && cancelled.ended);

        let marker =
            std::env::temp_dir().join(format!("navis-sync-marker-{}.tmp", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let body = format!(
            "from pathlib import Path\nPath(r'{}').write_text('spawned')\n",
            marker.display()
        );
        let pending = script(&body);
        let cancelled = super::run(
            "python",
            &[pending.to_string_lossy().into_owned()],
            Duration::from_secs(5),
            || true,
            |_, _| {},
        );
        let _ = std::fs::remove_file(pending);
        assert!(cancelled.cancelled && cancelled.ended);
        assert!(!marker.exists());
    }

    #[cfg(windows)]
    #[test]
    fn resume_failure_fault_hook_confirms_owned_job_cleanup_and_uncertainty() {
        let path =
            script("from pathlib import Path\nPath('resume-must-not-run').write_text('x')\n");
        TEST_RESUME_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
        let confirmed = super::run(
            "python",
            &[path.to_string_lossy().into_owned()],
            Duration::from_secs(5),
            || false,
            |_, _| {},
        );
        assert!(
            confirmed.ended
                && confirmed
                    .error
                    .as_deref()
                    .is_some_and(|v| v.contains("terminated"))
        );
        TEST_RESUME_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
        TEST_RESUME_UNCERTAIN.store(true, std::sync::atomic::Ordering::SeqCst);
        let uncertain = super::run(
            "python",
            &[path.to_string_lossy().into_owned()],
            Duration::from_secs(5),
            || false,
            |_, _| {},
        );
        let _ = std::fs::remove_file(path);
        assert!(
            !uncertain.ended
                && uncertain
                    .error
                    .as_deref()
                    .is_some_and(|v| v.contains("uncertain"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn runner_caps_noisy_lines_and_total_bytes() {
        let line = run_script("print('x' * 70000, flush=True)\n", Duration::from_secs(5));
        assert!(line.output_limited && line.ended);
        let total = run_script(
            "import sys\nsys.stdout.write('x' * (5 * 1024 * 1024))\nsys.stdout.flush()\n",
            Duration::from_secs(5),
        );
        assert!(total.output_limited && total.ended);
    }
}
