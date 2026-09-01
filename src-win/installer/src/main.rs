//! `claudepet-setup.exe` - a small self-contained install wizard for ClaudePet.
//!
//! The release `claudepet.exe` is embedded at build time (`include_bytes!`), so
//! this is the only file a user needs. Per-user install (no admin): copies the
//! app into `%LOCALAPPDATA%\Programs\ClaudePet`, creates Start-menu / desktop
//! shortcuts, optionally a sign-in autostart entry, and registers an uninstaller
//! in Add/Remove Programs. Run with `--uninstall` to reverse all of that.

#![windows_subsystem = "windows"]

use std::path::PathBuf;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::Graphics::Gdi::{HFONT, UpdateWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD,
    REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
    PROGRESS_CLASSW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::*;

const APP_EXE: &[u8] = include_bytes!("../../target/release/claudepet.exe");
const APP_NAME: &str = "ClaudePet";
const APP_VERSION: &str = "0.1.0";
const PUBLISHER: &str = "emm312";
const ARP_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ClaudePet";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

// control ids
const ID_BACK: isize = 1;
const ID_NEXT: isize = 2;
const ID_CANCEL: isize = 3;
const ID_BROWSE: isize = 4;
const ID_PATH: isize = 5;
const ID_CB_STARTMENU: isize = 6;
const ID_CB_DESKTOP: isize = 7;
const ID_CB_AUTOSTART: isize = 8;
const ID_CB_LAUNCH: isize = 9;

const PBM_SETRANGE32: u32 = WM_USER + 6;
const PBM_SETPOS: u32 = WM_USER + 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |names: &[&str]| args.iter().any(|a| names.iter().any(|n| a.eq_ignore_ascii_case(n)));
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let silent = has(&["--silent", "/S", "/silent"]);
        if has(&["--uninstall", "/uninstall"]) {
            run_uninstall(silent);
        } else if silent {
            // Unattended install with defaults (Start-menu + desktop shortcuts,
            // no autostart). Used for scripted deploys and CI smoke checks.
            let opt = Options {
                dir: default_install_dir(),
                start_menu: true,
                desktop: true,
                autostart: false,
            };
            match do_install(&opt, HWND::default()) {
                Ok(p) => println!("installed: {}", p.display()),
                Err(e) => {
                    eprintln!("install failed: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            run_wizard();
        }
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn default_install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    base.join("Programs").join(APP_NAME)
}

fn start_menu_lnk() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("Microsoft\\Windows\\Start Menu\\Programs")
            .join(format!("{APP_NAME}.lnk")),
    )
}

fn desktop_lnk() -> Option<PathBuf> {
    let up = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(up).join("Desktop").join(format!("{APP_NAME}.lnk")))
}

// ---------------------------------------------------------------------------
// COM shortcut
// ---------------------------------------------------------------------------

unsafe fn create_shortcut(lnk: &std::path::Path, target: &std::path::Path, workdir: &std::path::Path) {
    let make = || -> windows::core::Result<()> {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        link.SetWorkingDirectory(&HSTRING::from(workdir.as_os_str()))?;
        link.SetDescription(w!("ClaudePet desktop pet"))?;
        let pf: IPersistFile = windows::core::Interface::cast(&link)?;
        pf.Save(&HSTRING::from(lnk.as_os_str()), BOOL(1))?;
        Ok(())
    };
    if let Err(e) = make() {
        eprintln!("shortcut {lnk:?} failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

unsafe fn reg_set_sz(subkey: &str, name: &str, value: &str) {
    let data = wide(value);
    let _ = RegSetKeyValueW(
        HKEY_CURRENT_USER,
        &HSTRING::from(subkey),
        &HSTRING::from(name),
        REG_SZ.0,
        Some(data.as_ptr() as *const _),
        (data.len() * 2) as u32,
    );
}

unsafe fn reg_get_sz(subkey: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut cb = (buf.len() * 2) as u32;
    let status = RegGetValueW(
        HKEY_CURRENT_USER,
        &HSTRING::from(subkey),
        &HSTRING::from(name),
        RRF_RT_REG_SZ,
        None,
        Some(buf.as_mut_ptr() as *mut _),
        Some(&mut cb),
    );
    if status.is_err() || cb < 2 {
        return None;
    }
    let n = (cb as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..n]))
}

unsafe fn reg_set_dword(subkey: &str, name: &str, value: u32) {
    let _ = RegSetKeyValueW(
        HKEY_CURRENT_USER,
        &HSTRING::from(subkey),
        &HSTRING::from(name),
        REG_DWORD.0,
        Some(&value as *const u32 as *const _),
        4,
    );
}

// ---------------------------------------------------------------------------
// Install / uninstall actions
// ---------------------------------------------------------------------------

struct Options {
    dir: PathBuf,
    start_menu: bool,
    desktop: bool,
    autostart: bool,
}

/// Returns the installed app exe path on success.
unsafe fn do_install(opt: &Options, progress: HWND) -> std::io::Result<PathBuf> {
    let set = |p: i32| {
        let _ = SendMessageW(progress, PBM_SETPOS, WPARAM(p as usize), LPARAM(0));
    };

    std::fs::create_dir_all(&opt.dir)?;
    set(15);

    let app_path = opt.dir.join("claudepet.exe");
    std::fs::write(&app_path, APP_EXE)?;
    set(45);

    // Keep a copy of this setup binary as the uninstaller.
    if let Ok(me) = std::env::current_exe() {
        let _ = std::fs::copy(&me, opt.dir.join("uninstall.exe"));
    }
    set(60);

    if opt.start_menu {
        if let Some(lnk) = start_menu_lnk() {
            create_shortcut(&lnk, &app_path, &opt.dir);
        }
    }
    set(72);

    if opt.desktop {
        if let Some(lnk) = desktop_lnk() {
            create_shortcut(&lnk, &app_path, &opt.dir);
        }
    }
    set(82);

    if opt.autostart {
        reg_set_sz(RUN_KEY, APP_NAME, &format!("\"{}\"", app_path.display()));
    }
    set(90);

    let uninstall_cmd = format!("\"{}\" --uninstall", opt.dir.join("uninstall.exe").display());
    reg_set_sz(ARP_KEY, "DisplayName", APP_NAME);
    reg_set_sz(ARP_KEY, "DisplayVersion", APP_VERSION);
    reg_set_sz(ARP_KEY, "Publisher", PUBLISHER);
    reg_set_sz(ARP_KEY, "InstallLocation", &opt.dir.display().to_string());
    reg_set_sz(ARP_KEY, "DisplayIcon", &app_path.display().to_string());
    reg_set_sz(ARP_KEY, "UninstallString", &uninstall_cmd);
    reg_set_dword(ARP_KEY, "NoModify", 1);
    reg_set_dword(ARP_KEY, "NoRepair", 1);
    reg_set_dword(ARP_KEY, "EstimatedSize", (APP_EXE.len() / 1024) as u32);
    set(100);

    Ok(app_path)
}

unsafe fn run_uninstall(silent: bool) {
    if !silent {
        let answer = MessageBoxW(
            None,
            w!("Remove ClaudePet from this PC?\n\n(Your pet's saved state is kept.)"),
            w!("Uninstall ClaudePet"),
            MB_YESNO | MB_ICONQUESTION,
        );
        if answer != IDYES {
            return;
        }
    }

    // Stop a running instance.
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "claudepet.exe"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output();

    if let Some(lnk) = start_menu_lnk() {
        let _ = std::fs::remove_file(lnk);
    }
    if let Some(lnk) = desktop_lnk() {
        let _ = std::fs::remove_file(lnk);
    }
    // Resolve the install dir from the registry - never from this binary's own
    // location, so running the build copy with --uninstall can't wipe a source
    // tree.
    let dir = reg_get_sz(ARP_KEY, "InstallLocation")
        .map(PathBuf::from)
        .filter(|p| p.join("claudepet.exe").exists())
        .unwrap_or_else(default_install_dir);

    let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN_KEY), &HSTRING::from(APP_NAME));
    let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(ARP_KEY));

    if !dir.join("claudepet.exe").exists() {
        // Nothing recognisable to remove - stop before touching any directory.
        return;
    }

    // Delete the install directory (which holds this running uninstaller) from a
    // detached shell that waits for us to exit first. `raw_arg` so cmd.exe's
    // /C quoting rules get exactly the command line they expect.
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!(
            "/c \"ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"\"",
            dir.display()
        ))
        .creation_flags(0x0800_0000)
        .spawn();

    if !silent {
        MessageBoxW(
            None,
            w!("ClaudePet has been removed."),
            w!("Uninstall ClaudePet"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

use std::os::windows::process::CommandExt;

// ---------------------------------------------------------------------------
// Wizard window
// ---------------------------------------------------------------------------

struct Wiz {
    page: i32,
    finished: bool,
    installed_exe: Option<PathBuf>,
    font: HFONT,
    font_title: HFONT,

    title: HWND,
    subtitle: HWND,
    welcome: HWND,
    loc_label: HWND,
    path: HWND,
    browse: HWND,
    cb_startmenu: HWND,
    cb_desktop: HWND,
    cb_autostart: HWND,
    prog_label: HWND,
    progress: HWND,
    done_body: HWND,
    cb_launch: HWND,
    back: HWND,
    next: HWND,
    cancel: HWND,
    divider_top: HWND,
    divider_bottom: HWND,
}

const CW: i32 = 520;
const CH: i32 = 360;

unsafe fn run_wizard() {
    let hinst = GetModuleHandleW(None).unwrap();

    let icce = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES,
    };
    let _ = InitCommonControlsEx(&icce);

    let class = w!("ClaudePetSetupWnd");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: class,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
            (windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as isize as *mut _,
        ),
        hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let mut rc = windows::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: CW,
        bottom: CH,
    };
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let _ = AdjustWindowRectEx(&mut rc, style, false, WS_EX_DLGMODALFRAME);
    let (ww, wh) = (rc.right - rc.left, rc.bottom - rc.top);
    let (scr_w, scr_h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));

    let mut wiz = Box::new(Wiz {
        page: 0,
        finished: false,
        installed_exe: None,
        font: Default::default(),
        font_title: Default::default(),
        title: Default::default(),
        subtitle: Default::default(),
        welcome: Default::default(),
        loc_label: Default::default(),
        path: Default::default(),
        browse: Default::default(),
        cb_startmenu: Default::default(),
        cb_desktop: Default::default(),
        cb_autostart: Default::default(),
        prog_label: Default::default(),
        progress: Default::default(),
        done_body: Default::default(),
        cb_launch: Default::default(),
        back: Default::default(),
        next: Default::default(),
        cancel: Default::default(),
        divider_top: Default::default(),
        divider_bottom: Default::default(),
    });

    let hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        class,
        w!("ClaudePet Setup"),
        style,
        (scr_w - ww) / 2,
        (scr_h - wh) / 2,
        ww,
        wh,
        None,
        None,
        hinst,
        Some(&mut *wiz as *mut _ as *const _),
    )
    .expect("CreateWindowExW");

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if !IsDialogMessageW(hwnd, &msg).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn mk(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    ex: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    w_: i32,
    h: i32,
    id: isize,
    hinst: windows::Win32::Foundation::HMODULE,
) -> HWND {
    CreateWindowExW(
        ex,
        class,
        &HSTRING::from(text),
        style | WS_CHILD,
        x,
        y,
        w_,
        h,
        parent,
        HMENU(id as *mut _),
        hinst,
        None,
    )
    .unwrap_or_default()
}

unsafe fn set_font(h: HWND, f: HFONT) {
    SendMessageW(h, WM_SETFONT, WPARAM(f.0 as usize), LPARAM(1));
}

unsafe fn create_controls(hwnd: HWND, wiz: &mut Wiz) {
    let hinst = GetModuleHandleW(None).unwrap();
    use windows::Win32::Graphics::Gdi::{CreateFontW, FW_NORMAL, FW_SEMIBOLD};

    wiz.font = CreateFontW(-15, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, 0, 0, 0, 0, 0, w!("Segoe UI"));
    wiz.font_title =
        CreateFontW(-20, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, 0, 0, 0, 0, 0, w!("Segoe UI"));

    let st = WINDOW_STYLE(0);
    let vis = WS_VISIBLE;
    let s = |extra: u32| WINDOW_STYLE(extra) | vis;

    wiz.title = mk(hwnd, w!("STATIC"), "", vis, WINDOW_EX_STYLE(0), 24, 18, CW - 48, 26, 0, hinst);
    wiz.subtitle =
        mk(hwnd, w!("STATIC"), "", vis, WINDOW_EX_STYLE(0), 24, 44, CW - 48, 20, 0, hinst);
    wiz.divider_top = mk(
        hwnd,
        w!("STATIC"),
        "",
        s(0x10u32),
        WINDOW_EX_STYLE(0),
        0,
        72,
        CW,
        2,
        0,
        hinst,
    );

    // Page 0
    wiz.welcome = mk(
        hwnd,
        w!("STATIC"),
        "This will install ClaudePet on your computer.\r\n\r\nClaudePet is a small desktop pet that lives on top of your other windows, \
         walks around, perches on title bars, and can carry short messages to another \
         ClaudePet on your network.\r\n\r\nClick Next to continue.",
        st,
        WINDOW_EX_STYLE(0),
        28,
        92,
        CW - 56,
        180,
        0,
        hinst,
    );

    // Page 1
    wiz.loc_label = mk(
        hwnd,
        w!("STATIC"),
        "Install location:",
        st,
        WINDOW_EX_STYLE(0),
        28,
        92,
        CW - 56,
        18,
        0,
        hinst,
    );
    wiz.path = mk(
        hwnd,
        w!("EDIT"),
        &default_install_dir().display().to_string(),
        s(WS_TABSTOP.0 | ES_AUTOHSCROLL as u32),
        WS_EX_CLIENTEDGE,
        28,
        112,
        CW - 56 - 88,
        24,
        ID_PATH,
        hinst,
    );
    wiz.browse = mk(
        hwnd,
        w!("BUTTON"),
        "Browse\u{2026}",
        s(WS_TABSTOP.0),
        WINDOW_EX_STYLE(0),
        CW - 28 - 80,
        111,
        80,
        26,
        ID_BROWSE,
        hinst,
    );
    wiz.cb_startmenu = mk(
        hwnd,
        w!("BUTTON"),
        "Create a Start Menu shortcut",
        s(WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32),
        WINDOW_EX_STYLE(0),
        28,
        156,
        CW - 56,
        22,
        ID_CB_STARTMENU,
        hinst,
    );
    wiz.cb_desktop = mk(
        hwnd,
        w!("BUTTON"),
        "Create a desktop shortcut",
        s(WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32),
        WINDOW_EX_STYLE(0),
        28,
        182,
        CW - 56,
        22,
        ID_CB_DESKTOP,
        hinst,
    );
    wiz.cb_autostart = mk(
        hwnd,
        w!("BUTTON"),
        "Start ClaudePet when I sign in",
        s(WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32),
        WINDOW_EX_STYLE(0),
        28,
        208,
        CW - 56,
        22,
        ID_CB_AUTOSTART,
        hinst,
    );
    SendMessageW(wiz.cb_startmenu, BM_SETCHECK, WPARAM(1), LPARAM(0));
    SendMessageW(wiz.cb_desktop, BM_SETCHECK, WPARAM(1), LPARAM(0));

    // Page 2
    wiz.prog_label = mk(
        hwnd,
        w!("STATIC"),
        "Copying files\u{2026}",
        st,
        WINDOW_EX_STYLE(0),
        28,
        130,
        CW - 56,
        18,
        0,
        hinst,
    );
    wiz.progress = mk(
        hwnd,
        PROGRESS_CLASSW,
        "",
        st,
        WINDOW_EX_STYLE(0),
        28,
        152,
        CW - 56,
        22,
        0,
        hinst,
    );
    SendMessageW(wiz.progress, PBM_SETRANGE32, WPARAM(0), LPARAM(100));

    // Page 3
    wiz.done_body = mk(
        hwnd,
        w!("STATIC"),
        "ClaudePet has been installed.\r\n\r\nClick Finish to close this wizard.",
        st,
        WINDOW_EX_STYLE(0),
        28,
        92,
        CW - 56,
        120,
        0,
        hinst,
    );
    wiz.cb_launch = mk(
        hwnd,
        w!("BUTTON"),
        "Launch ClaudePet now",
        s(WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32),
        WINDOW_EX_STYLE(0),
        28,
        168,
        CW - 56,
        22,
        ID_CB_LAUNCH,
        hinst,
    );
    SendMessageW(wiz.cb_launch, BM_SETCHECK, WPARAM(1), LPARAM(0));

    // Footer
    wiz.divider_bottom = mk(
        hwnd,
        w!("STATIC"),
        "",
        s(0x10u32),
        WINDOW_EX_STYLE(0),
        0,
        CH - 48,
        CW,
        2,
        0,
        hinst,
    );
    wiz.back = mk(
        hwnd,
        w!("BUTTON"),
        "< Back",
        s(WS_TABSTOP.0),
        WINDOW_EX_STYLE(0),
        CW - 28 - 3 * 88,
        CH - 38,
        84,
        28,
        ID_BACK,
        hinst,
    );
    wiz.next = mk(
        hwnd,
        w!("BUTTON"),
        "Next >",
        s(WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32),
        WINDOW_EX_STYLE(0),
        CW - 28 - 2 * 88,
        CH - 38,
        84,
        28,
        ID_NEXT,
        hinst,
    );
    wiz.cancel = mk(
        hwnd,
        w!("BUTTON"),
        "Cancel",
        s(WS_TABSTOP.0),
        WINDOW_EX_STYLE(0),
        CW - 28 - 88,
        CH - 38,
        84,
        28,
        ID_CANCEL,
        hinst,
    );

    for h in [
        wiz.title,
        wiz.subtitle,
        wiz.welcome,
        wiz.loc_label,
        wiz.path,
        wiz.browse,
        wiz.cb_startmenu,
        wiz.cb_desktop,
        wiz.cb_autostart,
        wiz.prog_label,
        wiz.done_body,
        wiz.cb_launch,
        wiz.back,
        wiz.next,
        wiz.cancel,
    ] {
        set_font(h, wiz.font);
    }
    set_font(wiz.title, wiz.font_title);
}

unsafe fn show_page(wiz: &Wiz, page: i32) {
    let p0 = [wiz.welcome];
    let p1 = [
        wiz.loc_label,
        wiz.path,
        wiz.browse,
        wiz.cb_startmenu,
        wiz.cb_desktop,
        wiz.cb_autostart,
    ];
    let p2 = [wiz.prog_label, wiz.progress];
    let p3 = [wiz.done_body, wiz.cb_launch];
    for h in p0.iter().chain(&p1).chain(&p2).chain(&p3) {
        let _ = ShowWindow(*h, SW_HIDE);
    }
    let current: &[HWND] = match page {
        0 => &p0,
        1 => &p1,
        2 => &p2,
        _ => &p3,
    };
    for h in current {
        let _ = ShowWindow(*h, SW_SHOW);
    }

    let (title, sub) = match page {
        0 => ("Welcome to ClaudePet Setup", ""),
        1 => ("Choose install options", "Where should ClaudePet go?"),
        2 => ("Installing", "One moment\u{2026}"),
        _ => ("Installation complete", ""),
    };
    let _ = SetWindowTextW(wiz.title, &HSTRING::from(title));
    let _ = SetWindowTextW(wiz.subtitle, &HSTRING::from(sub));

    let en = |h: HWND, on: bool| {
        let _ = EnableWindow(h, on);
    };
    let vis = |h: HWND, on: bool| {
        let _ = ShowWindow(h, if on { SW_SHOW } else { SW_HIDE });
    };
    match page {
        0 => {
            en(wiz.back, false);
            vis(wiz.back, true);
            en(wiz.next, true);
            vis(wiz.next, true);
            let _ = SetWindowTextW(wiz.next, w!("Next >"));
            en(wiz.cancel, true);
            vis(wiz.cancel, true);
        }
        1 => {
            en(wiz.back, true);
            vis(wiz.back, true);
            en(wiz.next, true);
            vis(wiz.next, true);
            let _ = SetWindowTextW(wiz.next, w!("Install"));
            en(wiz.cancel, true);
            vis(wiz.cancel, true);
        }
        2 => {
            en(wiz.back, false);
            en(wiz.next, false);
            en(wiz.cancel, false);
        }
        _ => {
            vis(wiz.back, false);
            en(wiz.next, true);
            vis(wiz.next, true);
            let _ = SetWindowTextW(wiz.next, w!("Finish"));
            vis(wiz.cancel, false);
            let _ = SetFocus(wiz.next);
        }
    }
}

unsafe fn read_options(wiz: &Wiz) -> Options {
    let mut buf = [0u16; 1024];
    let n = GetWindowTextW(wiz.path, &mut buf);
    let dir = String::from_utf16_lossy(&buf[..n as usize]);
    let checked = |h: HWND| SendMessageW(h, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
    Options {
        dir: if dir.trim().is_empty() {
            default_install_dir()
        } else {
            PathBuf::from(dir.trim())
        },
        start_menu: checked(wiz.cb_startmenu),
        desktop: checked(wiz.cb_desktop),
        autostart: checked(wiz.cb_autostart),
    }
}

unsafe fn browse_for_folder(owner: HWND, wiz: &Wiz) {
    use windows::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };
    let title = wide("Select the ClaudePet install folder");
    let mut bi = BROWSEINFOW {
        hwndOwner: owner,
        lpszTitle: PCWSTR(title.as_ptr()),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        ..Default::default()
    };
    let pidl = SHBrowseForFolderW(&mut bi);
    if pidl.is_null() {
        return;
    }
    let mut path = [0u16; 260];
    if SHGetPathFromIDListW(pidl, &mut path).as_bool() {
        let picked = String::from_utf16_lossy(&path)
            .trim_end_matches('\0')
            .to_string();
        let full = PathBuf::from(picked).join(APP_NAME);
        let _ = SetWindowTextW(wiz.path, &HSTRING::from(full.display().to_string()));
    }
    windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lp.0 as *const CREATESTRUCTW;
            let wiz = (*cs).lpCreateParams as *mut Wiz;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, wiz as isize);
            (*wiz).page = 0;
            create_controls(hwnd, &mut *wiz);
            show_page(&*wiz, 0);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            use windows::Win32::Graphics::Gdi::{GetStockObject, SetBkMode, WHITE_BRUSH, TRANSPARENT};
            SetBkMode(windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut _), TRANSPARENT);
            LRESULT(GetStockObject(WHITE_BRUSH).0 as isize)
        }
        WM_COMMAND => {
            let wiz_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Wiz;
            if wiz_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let id = (wp.0 & 0xffff) as isize;
            match id {
                ID_BROWSE => browse_for_folder(hwnd, &*wiz_ptr),
                ID_BACK => {
                    let wiz = &mut *wiz_ptr;
                    if wiz.page == 1 {
                        wiz.page = 0;
                        show_page(wiz, 0);
                    }
                }
                ID_NEXT => on_next(hwnd, wiz_ptr),
                ID_CANCEL => {
                    let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let wiz_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Wiz;
            if !wiz_ptr.is_null() {
                let wiz = &*wiz_ptr;
                if !wiz.finished && wiz.page < 3 {
                    let r = MessageBoxW(
                        hwnd,
                        w!("Cancel the ClaudePet installation?"),
                        w!("ClaudePet Setup"),
                        MB_YESNO | MB_ICONQUESTION,
                    );
                    if r != IDYES {
                        return LRESULT(0);
                    }
                }
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn on_next(hwnd: HWND, wiz_ptr: *mut Wiz) {
    let wiz = &mut *wiz_ptr;
    match wiz.page {
        0 => {
            wiz.page = 1;
            show_page(wiz, 1);
        }
        1 => {
            wiz.page = 2;
            show_page(wiz, 2);
            let _ = UpdateWindow(hwnd);
            let opt = read_options(wiz);
            match do_install(&opt, wiz.progress) {
                Ok(exe) => {
                    wiz.installed_exe = Some(exe);
                    wiz.page = 3;
                    show_page(wiz, 3);
                }
                Err(e) => {
                    MessageBoxW(
                        hwnd,
                        &HSTRING::from(format!("Install failed:\n{e}")),
                        w!("ClaudePet Setup"),
                        MB_OK | MB_ICONERROR,
                    );
                    wiz.page = 1;
                    show_page(wiz, 1);
                }
            }
        }
        _ => {
            let launch =
                SendMessageW(wiz.cb_launch, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
            if launch {
                if let Some(exe) = &wiz.installed_exe {
                    let _ = std::process::Command::new(exe).spawn();
                }
            }
            wiz.finished = true;
            let _ = DestroyWindow(hwnd);
        }
    }
}
