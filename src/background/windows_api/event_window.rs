use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::{
        DataExchange::AddClipboardFormatListener,
        Power::RegisterSuspendResumeNotification,
        RemoteDesktop::{NOTIFY_FOR_ALL_SESSIONS, WTSRegisterSessionNotification},
    },
    UI::{
        Shell::{
            Common::ITEMIDLIST, SHCNRF_SOURCE, SHChangeNotifyEntry, SHChangeNotifyRegister,
            SHGetSpecialFolderLocation,
        },
        WindowsAndMessaging::{
            ChangeWindowMessageFilterEx, CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE,
            DefWindowProcW, DispatchMessageW, GetMessageW, MSG, MSGFLT_ALLOW, PostQuitMessage,
            RegisterClassW, RegisterShellHookWindow, RegisterWindowMessageW, TranslateMessage,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_DESTROY,
            WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
        },
    },
};

use crate::{
    error::{Result, ResultLogExt, WindowsResultExt},
    event_manager,
    platform::session,
    utils::spawn_named_thread,
    widgets::manager::WIDGET_MANAGER,
};

use super::{WindowsApi, string_utils::WindowsString};

pub static WM_SHELLHOOKMESSAGE: AtomicU32 = AtomicU32::new(u32::MAX);
// pub const HSHELL_FULLSCREEN_ENTER: u32 = 53;
// pub const HSHELL_FULLSCREEN_EXIT: u32 = 54;

pub static BACKGROUND_HWND: AtomicIsize = AtomicIsize::new(0);

/// Window message sent to the background window when the recycle bin state changes.
pub const WM_TRASH_BIN_NOTIFY: u32 = WM_APP + 100;

/// CSIDL for the Recycle Bin virtual folder.
const CSIDL_BITBUCKET: i32 = 0x000a;
/// SHCNRF_ShellLevel
const SHCNRF_SHELL_LEVEL: SHCNRF_SOURCE = SHCNRF_SOURCE(0x0002);
/// SHCNE_ALLEVENTS
const SHCNE_ALL_EVENTS: i32 = 0x7FFFFFFF_u32 as i32;

pub struct BgWindowProc {}

event_manager!(BgWindowProc, (u32, usize, isize));

impl BgWindowProc {
    /// will lock until the window is closed
    unsafe fn _create_background_window(done: &crossbeam_channel::Sender<()>) -> Result<()> {
        unsafe {
            let title = WindowsString::from("Seelen UI Background Window");
            let class = WindowsString::from("SeelenBackgroundWindow");

            let h_module = WindowsApi::module_handle_w()?;

            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(Self::window_proc),
                hInstance: h_module.into(),
                lpszClassName: class.as_pcwstr(),
                ..Default::default()
            };

            RegisterClassW(&wnd_class);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.as_pcwstr(),
                title.as_pcwstr(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(wnd_class.hInstance),
                None,
            )?;

            let handle: isize = hwnd.0 as isize;
            BACKGROUND_HWND.store(handle, Ordering::Relaxed);

            // register window to recieve shell events
            {
                RegisterShellHookWindow(hwnd).ok().filter_fake_error()?;
                let msg = WindowsString::from("SHELLHOOK");
                WM_SHELLHOOKMESSAGE
                    .store(RegisterWindowMessageW(msg.as_pcwstr()), Ordering::Relaxed);
            }

            // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registersuspendresumenotification
            let _resume_suspend_handle =
                RegisterSuspendResumeNotification(hwnd.into(), DEVICE_NOTIFY_WINDOW_HANDLE)?;

            // Register for session change notifications (lock/unlock, user switch, etc.)
            // This is critical for pausing background threads when session is not interactive
            WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_ALL_SESSIONS)?;

            // Register to receive WM_CLIPBOARDUPDATE whenever the clipboard content
            // changes. This is the reliable, package-identity-independent way to detect
            // clipboard changes; the WinRT `Clipboard.ContentChanged` event does not
            // fire for unpackaged Win32 apps like this one.
            //
            // If this process ever runs elevated, UIPI blocks WM_CLIPBOARDUPDATE from
            // lower-integrity processes (e.g. a normal copy in Notepad) unless we
            // explicitly allow it through the message filter.
            match AddClipboardFormatListener(hwnd) {
                Ok(()) => log::debug!("[event_window] registered clipboard format listener"),
                Err(e) => log::error!("[event_window] AddClipboardFormatListener failed: {e}"),
            }
            ChangeWindowMessageFilterEx(hwnd, WM_CLIPBOARDUPDATE, MSGFLT_ALLOW, None).log_error();

            // Register for shell change notifications so that WM_TRASH_BIN_NOTIFY is sent
            // to the background window whenever the recycle bin contents change.
            register_shell_notifications(hwnd);

            done.send(())?;
            let mut msg = MSG::default();

            // GetMessageW will run until PostQuitMessage(0) is called
            while GetMessageW(&mut msg, Some(hwnd), 0, 0).into() {
                TranslateMessage(&msg).ok().filter_fake_error()?;
                DispatchMessageW(&msg);
            }
            Ok(())
        }
    }

    fn refresh_interactive_state() {
        let Some(active_session) = slu_ipc::app::current_interactive_session_id() else {
            // no active session, assume lock screen
            session::set_interactive(false);
            return;
        };

        let my_session = slu_ipc::app::current_session_id().unwrap_or(u32::MAX);
        let is_interactive = active_session == my_session && !session::is_locked();

        log::info!(
            "Active session: {}, my session: {}, is interactive: {}",
            active_session,
            my_session,
            is_interactive
        );

        let previous = session::set_interactive(is_interactive);
        if !previous && is_interactive {
            // reload all pods webviews on session resume
            std::thread::spawn(move || {
                WIDGET_MANAGER.deployments.for_each(|(_, deployment)| {
                    deployment.pods.for_each(|(_, pod)| {
                        pod.soft_restart();
                    });
                });
            });
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        unsafe {
            if msg == WM_DESTROY {
                PostQuitMessage(0);
                return LRESULT(0);
            }

            if msg == WM_SHELLHOOKMESSAGE.load(Ordering::Relaxed)
                && w_param.0 as u32 == HSHELL_GETMINRECT
                && l_param.0 != 0
                && let Some(rect) = minimize_target(&*(l_param.0 as *const ShellHookInfo))
            {
                (*(l_param.0 as *mut ShellHookInfo)).rc = rect;
                return LRESULT(1);
            }

            // Handle session change notifications to pause background processing
            // when the session is locked or user switches
            if msg == WM_WTSSESSION_CHANGE {
                log::trace!(
                    "Session change event received: w_param={}, l_param={}",
                    w_param.0,
                    l_param.0
                );

                match w_param.0 as u32 {
                    WTS_SESSION_LOCK
                        if l_param.0 as u32
                            == slu_ipc::app::current_session_id().unwrap_or(u32::MAX) =>
                    {
                        session::set_locked(true);
                        log::info!("Session locked");
                    }
                    WTS_SESSION_UNLOCK
                        if l_param.0 as u32
                            == slu_ipc::app::current_session_id().unwrap_or(u32::MAX) =>
                    {
                        session::set_locked(false);
                        log::info!("Session unlocked");
                    }
                    _ => {}
                }

                Self::refresh_interactive_state();
            }

            Self::send((msg, w_param.0, l_param.0));
            DefWindowProcW(hwnd, msg, w_param, l_param)
        }
    }
}

/// Asked by Windows where a window being minimized shrinks to (`lParam` of `HSHELL_GETMINRECT`).
const HSHELL_GETMINRECT: u32 = 4;

#[repr(C)]
struct ShellHookInfo {
    hwnd: HWND,
    rc: RECT,
}

/// Where minimized windows shrink to when the dock is not at the bottom of the screen, as the
/// hidden Windows taskbar would make them go down: the middle of the dock's edge of the window's
/// monitor. `None` keeps the default.
fn minimize_target(info: &ShellHookInfo) -> Option<RECT> {
    use seelen_core::state::SeelenWegSide;

    let weg = crate::state::FULL_STATE.load().settings.by_widget.weg.clone();
    if !weg.enabled || weg.position == SeelenWegSide::Bottom {
        return None;
    }
    let monitor = WindowsApi::monitor_rect(WindowsApi::monitor_from_window(info.hwnd)).ok()?;
    let (center_x, center_y) = (
        (monitor.left + monitor.right) / 2,
        (monitor.top + monitor.bottom) / 2,
    );
    const HALF: i32 = 24;
    const DEPTH: i32 = 48;
    Some(match weg.position {
        SeelenWegSide::Top => RECT {
            left: center_x - HALF,
            top: monitor.top,
            right: center_x + HALF,
            bottom: monitor.top + DEPTH,
        },
        SeelenWegSide::Left => RECT {
            left: monitor.left,
            top: center_y - HALF,
            right: monitor.left + DEPTH,
            bottom: center_y + HALF,
        },
        SeelenWegSide::Right => RECT {
            left: monitor.right - DEPTH,
            top: center_y - HALF,
            right: monitor.right,
            bottom: center_y + HALF,
        },
        SeelenWegSide::Bottom => return None,
    })
}

/// Registers shell change notifications for the Recycle Bin on the background window.
/// When the recycle bin contents change, Windows sends `WM_TRASH_BIN_NOTIFY` to `hwnd`,
/// which is then broadcast to all `subscribe_to_background_window` subscribers.
unsafe fn register_shell_notifications(hwnd: HWND) {
    unsafe {
        let pidl: *mut ITEMIDLIST =
            SHGetSpecialFolderLocation(None, CSIDL_BITBUCKET).unwrap_or(std::ptr::null_mut());

        let entry = SHChangeNotifyEntry {
            pidl,
            fRecursive: true.into(),
        };

        SHChangeNotifyRegister(
            hwnd,
            SHCNRF_SHELL_LEVEL,
            SHCNE_ALL_EVENTS,
            WM_TRASH_BIN_NOTIFY,
            1,
            &entry,
        );
    }
}

/// the objective with this window is having a thread that will receive window events
/// and propagate them across the application (common events are keyboard, power, display, etc)
pub fn create_background_window() -> Result<()> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    spawn_named_thread("Background Window", move || {
        log::trace!("Creating background window...");
        unsafe { BgWindowProc::_create_background_window(&tx) }.log_error();
    });
    rx.recv()?;
    log::trace!("Background window created");
    Ok(())
}

pub fn subscribe_to_background_window<F>(callback: F)
where
    F: Fn(u32, usize, isize) -> Result<()> + Send + Sync + 'static,
{
    BgWindowProc::subscribe(move |arg| {
        callback(arg.0, arg.1, arg.2).log_error();
    });
}
