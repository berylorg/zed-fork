use super::*;
use windows::Win32::System::Threading::GetCurrentThreadId;

thread_local! {
    static DRIVER: RefCell<Option<DialogDriver>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
enum Response {
    Confirm,
    DeclineThenNew,
    DeclineDeleteThenSame,
    Cancel,
}

struct DialogDriver {
    dialog: IFileSaveDialog,
    response: Response,
    new_name: PathBuf,
    existing: PathBuf,
    deleted: bool,
    step: u8,
    ticks: usize,
    timed_out: bool,
}

unsafe extern "system" fn collect_window(window: HWND, context: LPARAM) -> BOOL {
    unsafe {
        (&mut *(context.0 as *mut Vec<HWND>)).push(window);
    }
    true.into()
}

fn press_button(window: HWND, id: i32) -> bool {
    let mut children: Vec<HWND> = Vec::new();
    let _ = unsafe {
        EnumChildWindows(
            Some(window),
            Some(collect_window),
            LPARAM(&mut children as *mut _ as isize),
        )
    };
    let buttons: Vec<_> = children
        .iter()
        .copied()
        .filter(|child| {
            let mut class = [0u16; 64];
            let length = unsafe { GetClassNameW(*child, &mut class) } as usize;
            (unsafe { GetDlgCtrlID(*child) == id && IsWindowEnabled(*child).as_bool() })
                && String::from_utf16_lossy(&class[..length]).eq_ignore_ascii_case("button")
        })
        .collect();
    let button = buttons.iter().copied().max_by_key(|button| {
        let mut rect = RECT::default();
        let _ = unsafe { GetWindowRect(*button, &mut rect) };
        rect.bottom
    });
    if let Some(button) = button {
        let mut caption = [0u16; 32];
        let length = unsafe { GetWindowTextW(button, &mut caption) } as usize;
        eprintln!(
            "owned native button: id={} matches={} caption={}",
            id,
            buttons.len(),
            String::from_utf16_lossy(&caption[..length])
        );
        let parent = unsafe { GetParent(button) }
            .ok()
            .filter(|parent| *parent == window || unsafe { IsChild(window, *parent).as_bool() })
            .unwrap_or(window);
        eprintln!("owned native button parent is dialog: {}", parent == window);
        unsafe {
            let _ = SetForegroundWindow(window);
            let _ = SetActiveWindow(window);
            let _ = SetFocus(Some(button));
        }
        let _ = unsafe {
            PostMessageW(
                Some(parent),
                WM_COMMAND,
                WPARAM(id as usize),
                LPARAM(button.0 as isize),
            )
        };
        true
    } else {
        if matches!(id, 6 | 7) {
            let _ = unsafe {
                PostMessageW(
                    Some(window),
                    windows::Win32::UI::Controls::TDM_CLICK_BUTTON.0 as u32,
                    WPARAM(id as usize),
                    LPARAM(0),
                )
            };
            true
        } else {
            false
        }
    }
}

unsafe extern "system" fn drive_dialog(_: HWND, _: u32, _: usize, _: u32) {
    DRIVER.with(|driver| {
        let mut driver = driver.borrow_mut();
        let Some(driver) = driver.as_mut() else {
            return;
        };
        driver.ticks += 1;
        let Ok(ole) = driver.dialog.cast::<IOleWindow>() else {
            return;
        };
        let Ok(window) = (unsafe { ole.GetWindow() }) else {
            return;
        };
        let mut windows: Vec<HWND> = Vec::new();
        let _ = unsafe {
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(collect_window),
                LPARAM(&mut windows as *mut _ as isize),
            )
        };
        let confirmation = windows.into_iter().find(|candidate| {
            *candidate != window
                && unsafe { IsWindowVisible(*candidate).as_bool() }
                && unsafe { GetWindow(*candidate, GW_OWNER) }.ok() == Some(window)
        });
        if matches!(driver.ticks, 1 | 5 | 15 | 30) {
            eprintln!(
                "owned native dialog driver tick={} step={} confirmation={}",
                driver.ticks,
                driver.step,
                confirmation.is_some()
            );
        }
        if driver.ticks > 60 {
            driver.timed_out = true;
            if let Some(confirmation) = confirmation {
                press_button(confirmation, IDNO.0);
            }
            let _ = unsafe { driver.dialog.Close(HRESULT::from_win32(ERROR_CANCELLED.0)) };
            return;
        }
        if matches!(driver.response, Response::Cancel) {
            let _ = unsafe {
                PostMessageW(
                    Some(window),
                    WM_COMMAND,
                    WPARAM(IDCANCEL.0 as usize),
                    LPARAM(0),
                )
            };
            return;
        }
        match driver.step {
            0 => {
                if driver.ticks < 5 {
                    return;
                }
                if let Ok(name) = unsafe { driver.dialog.GetFileName() } {
                    let value = unsafe { name.to_string() };
                    unsafe {
                        CoTaskMemFree(Some(name.0 as _));
                    }
                    eprintln!(
                        "owned native filename length: {}",
                        value.as_ref().map_or(0, String::len)
                    );
                }
                if press_button(window, IDOK.0) {
                    driver.step = 1;
                }
            }
            1 => {
                if let Some(confirmation) = confirmation {
                    let response = if matches!(driver.response, Response::Confirm) {
                        IDYES
                    } else {
                        IDNO
                    };
                    if press_button(confirmation, response.0) {
                        driver.step = 2;
                    }
                }
            }
            2 if matches!(
                driver.response,
                Response::DeclineThenNew | Response::DeclineDeleteThenSame
            ) && confirmation.is_none() =>
            {
                if matches!(driver.response, Response::DeclineDeleteThenSame) {
                    if !driver.deleted {
                        std::fs::remove_file(&driver.existing).unwrap();
                        driver.deleted = true;
                    }
                    if press_button(window, IDOK.0) {
                        driver.step = 3;
                    }
                } else if unsafe {
                    driver
                        .dialog
                        .SetFileName(&HSTRING::from(driver.new_name.to_string_lossy().as_ref()))
                }
                .is_ok()
                {
                    if press_button(window, IDOK.0) {
                        driver.step = 3;
                    }
                }
            }
            _ => {}
        }
    });
}

struct NativeScope {
    timer: usize,
}
impl Drop for NativeScope {
    fn drop(&mut self) {
        if self.timer != 0 {
            let _ = unsafe { KillTimer(None, self.timer) };
        }
        DRIVER.with(|driver| driver.borrow_mut().take());
        unsafe {
            CoUninitialize();
        }
    }
}

fn native_choice(
    directory: &Path,
    name: &str,
    response: Response,
) -> Result<Option<SavePathChoice>> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let mut scope = NativeScope { timer: 0 };
    let dialog: IFileSaveDialog = unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL)? };
    DRIVER.with(|driver| {
        *driver.borrow_mut() = Some(DialogDriver {
            dialog: dialog.clone(),
            response,
            new_name: directory.join("new.txt"),
            existing: directory.join(name),
            deleted: false,
            step: 0,
            ticks: 0,
            timed_out: false,
        })
    });
    let timer = unsafe { SetTimer(None, 0, 100, Some(drive_dialog)) };
    scope.timer = timer;
    assert_ne!(timer, 0);
    let result =
        file_save_choice_with_dialog(dialog, directory.to_path_buf(), Some(name.to_owned()), None);
    let timed_out = DRIVER.with(|driver| driver.borrow().as_ref().unwrap().timed_out);
    drop(scope);
    assert!(
        !timed_out,
        "owned native save dialog must complete within six seconds"
    );
    result
}

fn fixture() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("native-save-choice-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    if let Some(ledger) = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER") {
        let mut ledger = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger)
            .unwrap();
        std::io::Write::write_all(&mut ledger, format!("{}\n", path.display()).as_bytes()).unwrap();
    }
    eprintln!("retained native save-choice fixture: {}", path.display());
    path
}

#[::core::prelude::v1::test]
fn native_save_choice_qualifies_confirmed_overwrite_and_declined_then_new_selection() {
    let path = fixture();
    std::fs::write(path.join("existing.txt"), b"original").unwrap();
    let existing = native_choice(&path, "existing.txt", Response::Confirm)
        .unwrap()
        .unwrap();
    assert_eq!(existing.path, path.join("existing.txt"));
    assert!(existing.replace_existing);
    let new = native_choice(&path, "existing.txt", Response::DeclineThenNew)
        .unwrap()
        .unwrap();
    assert_eq!(new.path, path.join("new.txt"));
    assert!(!new.replace_existing);
    assert_eq!(
        std::fs::read(path.join("existing.txt")).unwrap(),
        b"original"
    );
    assert!(!path.join("new.txt").exists());
    let same = native_choice(&path, "existing.txt", Response::DeclineDeleteThenSame)
        .unwrap()
        .unwrap();
    assert_eq!(same.path, path.join("existing.txt"));
    assert!(!same.replace_existing);
    assert!(!same.path.exists());
    assert!(
        native_choice(&path, "new.txt", Response::Cancel)
            .unwrap()
            .is_none()
    );
}

#[::core::prelude::v1::test]
fn save_dialog_show_only_cancelled_hresult_is_quiet_cancellation() {
    assert!(
        !save_dialog_show_result(Err(windows::core::Error::from_hresult(
            HRESULT::from_win32(ERROR_CANCELLED.0)
        )))
        .unwrap()
    );
    assert!(save_dialog_show_result(Err(windows::core::Error::from_hresult(E_FAIL))).is_err());
    assert!(save_dialog_show_result(Ok(())).unwrap());
}

#[::core::prelude::v1::test]
fn save_dialog_callback_failure_remains_failure_when_user_later_cancels() {
    assert!(
        save_dialog_outcome(
            Err(windows::core::Error::from_hresult(HRESULT::from_win32(
                ERROR_CANCELLED.0
            ))),
            true
        )
        .is_err()
    );
    assert!(save_dialog_outcome(Ok(()), true).is_err());
}
