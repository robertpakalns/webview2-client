#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
    },
    core::*,
};

pub fn create_utf_string(str: &str) -> Vec<u16> {
    let mut s: Vec<u16> = str.encode_utf16().collect();
    s.push(0);
    s
}

pub fn create_window() -> HWND {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let icon = match LoadIconW(Some(hinstance), w!("icon")) {
            Ok(icon) => icon,
            Err(_) => LoadIconW(None, IDI_APPLICATION).unwrap(),
        };

        let class_name = w!("webview2_client");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc_setup),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: icon,
            hCursor: Default::default(),
            hbrBackground: CreateSolidBrush(COLORREF(0x00000000)),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
        };

        RegisterClassW(&wc);

        let width = 800;
        let height = 600;

        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        let hwnd: HWND = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("WebView2 Client"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(hinstance),
            None,
        )
        .unwrap();

        hwnd
    }
}

pub fn create_webview2(hwnd: HWND) -> (ICoreWebView2Controller4, ICoreWebView2_22) {
    unsafe {
        let options: CoreWebView2EnvironmentOptions = CoreWebView2EnvironmentOptions::default();
        options.set_additional_browser_arguments("--disable-frame-rate-limit".to_string());

        let (tx, rx) = std::sync::mpsc::channel();
        let mut current_exe = std::env::current_exe().unwrap();
        current_exe.pop();

        let mut exe_dir = std::env::current_exe().unwrap();
        exe_dir.pop();

        let result = CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |environment_created_handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR::null(),
                    &ICoreWebView2EnvironmentOptions::from(options),
                    &environment_created_handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error_code, env| {
                error_code?;

                CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
                    Box::new(move |controller_created_handler| {
                        env.unwrap()
                            .CreateCoreWebView2Controller(hwnd, &controller_created_handler)
                            .map_err(webview2_com::Error::WindowsError)
                    }),
                    Box::new(move |err, controller| {
                        err?;
                        let controller = controller.unwrap();

                        let mut rect = RECT::default();
                        GetClientRect(hwnd, &mut rect).ok();
                        controller.SetBounds(rect).ok();

                        tx.send(controller).expect("error sending controller");
                        Ok(())
                    }),
                )
                .unwrap();
                Ok(())
            }),
        );

        if result.is_err() {
            panic!("Failed to create WebView2 env: {:?}", result)
        };

        let controller = rx
            .recv()
            .unwrap()
            .cast::<ICoreWebView2Controller4>()
            .unwrap();
        let webview2 = controller
            .CoreWebView2()
            .unwrap()
            .cast::<ICoreWebView2_22>()
            .unwrap();

        (controller, webview2)
    }
}

unsafe extern "system" fn wnd_proc_setup(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc_main as isize);
            return wnd_proc_main(hwnd, msg, wparam, lparam);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn wnd_proc_main(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            unsafe {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ICoreWebView2Controller4;
                if !ptr.is_null() {
                    let mut rect = RECT::default();
                    let controller = &*ptr;
                    GetClientRect(hwnd, &mut rect).ok();
                    controller.SetBounds(rect).ok();
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn main() {
    unsafe {
        let hwnd = create_window();
        let (controller, webview) = create_webview2(hwnd);

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &controller as *const _ as isize);

        webview
            .AddScriptToExecuteOnDocumentCreated(
                PCWSTR(create_utf_string(include_str!("script.js")).as_ptr()),
                None,
            )
            .ok();

        webview.Navigate(w!("https://kirka.io")).ok();

        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(
                    move |_, args: Option<ICoreWebView2WebMessageReceivedEventArgs>| {
                        let args = args.unwrap();
                        let mut message_vec = create_utf_string("");
                        let message = message_vec.as_mut_ptr() as *mut PWSTR;
                        args.TryGetWebMessageAsString(message).ok();
                        let message_string = message.as_ref().unwrap().to_string().unwrap();

                        let parts: Vec<&str> =
                            message_string.split(", ").map(|s| s.trim()).collect();
                        if parts.first() == Some(&"close") {
                            PostQuitMessage(0);
                        }
                        Ok(())
                    },
                )),
                0 as *mut i64,
            )
            .ok();

        let mut msg: MSG = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
