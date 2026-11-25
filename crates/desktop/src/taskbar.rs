use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{
            Shell::{
                DefSubclassProc, GetWindowSubclass, ITaskbarList3, RemoveWindowSubclass,
                SetWindowSubclass, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TaskbarList,
            },
            WindowsAndMessaging::{RegisterWindowMessageW, WM_NCDESTROY},
        },
    },
    core::{Result, w},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Progress {
    Hidden,
    Waiting,
    Bytes(u64, u64),
}

// The shell owns the button and can recreate it when Explorer restarts. Keep
// its creation count in the subclass data, without borrowing any Rust object.
unsafe extern "system" fn taskbar_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    generation: usize,
) -> LRESULT {
    unsafe {
        if message as usize == id {
            let _ = SetWindowSubclass(
                hwnd,
                Some(taskbar_message),
                id,
                generation.saturating_add(1),
            );
        } else if message == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(hwnd, Some(taskbar_message), id);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

pub(super) struct DownloadButton {
    hwnd: HWND,
    message: usize,
    generation: usize,
    shell: Option<ITaskbarList3>,
    desired: Progress,
    shown: Progress,
}

impl DownloadButton {
    pub(super) fn attach(window: &gpui::Window) -> Option<Self> {
        let RawWindowHandle::Win32(raw) = HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        Self::attach_hwnd(HWND(raw.hwnd.get() as _))
    }

    fn attach_hwnd(hwnd: HWND) -> Option<Self> {
        unsafe {
            let message = RegisterWindowMessageW(w!("TaskbarButtonCreated")) as usize;
            if message == 0 || !SetWindowSubclass(hwnd, Some(taskbar_message), message, 0).as_bool()
            {
                return None;
            }
            Some(Self {
                hwnd,
                message,
                generation: 0,
                shell: None,
                desired: Progress::Hidden,
                shown: Progress::Hidden,
            })
        }
    }

    pub(super) fn start(&mut self) {
        self.desired = Progress::Waiting;
    }

    pub(super) fn progress(&mut self, done: u64, total: u64) {
        if self.desired != Progress::Hidden && total > 0 {
            self.desired = Progress::Bytes(done, total);
        }
    }

    pub(super) fn finish(&mut self) {
        self.desired = Progress::Hidden;
    }

    // Called by the UI timer, including when the window is minimized. Coalesce
    // worker events per tick and send only changed values to the shell.
    pub(super) fn sync(&mut self) {
        if self.desired == Progress::Hidden && self.shown == Progress::Hidden {
            return;
        }
        let mut generation = 0;
        unsafe {
            if !GetWindowSubclass(
                self.hwnd,
                Some(taskbar_message),
                self.message,
                Some(&mut generation),
            )
            .as_bool()
                || generation == 0
            {
                return;
            }
        }
        if generation != self.generation {
            self.generation = generation;
            self.shown = Progress::Hidden;
            self.shell = None;
        }
        if self.desired != self.shown {
            if self.publish().is_ok() {
                self.shown = self.desired;
            } else {
                self.shell = None;
            }
        }
    }

    fn publish(&mut self) -> Result<()> {
        // GPUI initializes OLE on this window's thread before creating the HWND.
        unsafe {
            if self.shell.is_none() {
                let shell: ITaskbarList3 =
                    CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
                shell.HrInit()?;
                self.shell = Some(shell);
            }
            let shell = self.shell.as_ref().unwrap();
            match self.desired {
                Progress::Hidden => shell.SetProgressState(self.hwnd, TBPF_NOPROGRESS),
                Progress::Waiting => shell.SetProgressState(self.hwnd, TBPF_INDETERMINATE),
                Progress::Bytes(done, total) => shell.SetProgressValue(self.hwnd, done, total),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW, WS_OVERLAPPEDWINDOW,
    };

    #[test]
    fn download_follows_the_window_button_across_shell_recreation() -> Result<()> {
        unsafe {
            let hwnd = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("Rikai test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )?;
            let mut button = DownloadButton::attach_hwnd(hwnd).unwrap();
            button.progress(100, 300);
            assert_eq!(button.desired, Progress::Hidden);
            button.start();
            button.progress(0, 0);
            assert_eq!(button.desired, Progress::Waiting);
            button.progress(100, 300);
            button.sync();
            assert_eq!(button.generation, 0);
            for generation in 1..=2 {
                SendMessageW(hwnd, button.message as u32, None, None);
                button.sync();
                assert_eq!(button.generation, generation);
                assert_eq!(button.desired, Progress::Bytes(100, 300));
            }
            button.finish();
            button.progress(200, 300);
            assert_eq!(button.desired, Progress::Hidden);
            DestroyWindow(hwnd)
        }
    }
}
