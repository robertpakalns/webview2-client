#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use webview2_com::{
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
        ICoreWebView2EnvironmentOptions,
    },
    WebMessageReceivedEventHandler,
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GWLP_WNDPROC,
            GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, MSG, PostQuitMessage,
            RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SetWindowLongPtrW, TranslateMessage,
            WINDOW_EX_STYLE, WM_DESTROY, WM_NCCREATE, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

fn create_window() -> HWND {
    unsafe {
        let hinstance = HINSTANCE::default();

        let class_name = w!("webview2_client");
        let wc = WNDCLASSW {
            style: Default::default(),
            lpfnWndProc: Some(wnd_proc_setup),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: Default::default(),
            hCursor: Default::default(),
            hbrBackground: Default::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
        };

        RegisterClassW(&wc);

        let width = 800;
        let height = 600;

        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        CreateWindowExW(
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
        .unwrap()
    }
}

fn create_webview2(hwnd: HWND) -> (ICoreWebView2Controller, ICoreWebView2) {
    unsafe {
        let options = CoreWebView2EnvironmentOptions::default();
        options.set_additional_browser_arguments("--disable-frame-rate-limit".to_string());

        let (tx, rx) = std::sync::mpsc::channel();

        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR::null(),
                    &ICoreWebView2EnvironmentOptions::from(options),
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |err, env| {
                err?;

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
        )
        .expect("Failed to create WebView2 env");

        let controller = rx.recv().unwrap();
        let webview2 = controller.CoreWebView2().unwrap();

        (controller, webview2)
    }
}

extern "system" fn wnd_proc_setup(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc_main as isize);
            return wnd_proc_main(hwnd, msg, wparam, lparam);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

extern "system" fn wnd_proc_main(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            unsafe {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ICoreWebView2Controller;
                if !ptr.is_null() {
                    let mut rect = RECT::default();
                    GetClientRect(hwnd, &mut rect).ok();
                    (*ptr).SetBounds(rect).ok();
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

        let script = HSTRING::from(include_str!("script.js"));
        webview
            .AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), None)
            .ok();

        webview.Navigate(w!("https://kirka.io")).ok();

        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                    let mut msg = PWSTR::null();
                    args.unwrap().TryGetWebMessageAsString(&mut msg).ok();
                    if msg.to_string()?.trim_matches('"') == "close" {
                        PostQuitMessage(0);
                    }
                    Ok(())
                })),
                std::ptr::null_mut(),
            )
            .ok();

        let mut msg: MSG = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
