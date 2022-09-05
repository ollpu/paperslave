use std::{
    error::Error,
    ffi::{c_void, CString},
    ptr,
    sync::mpsc,
};

use esp_idf_hal::cpu::Core;
use esp_idf_sys::{vTaskDelete, xTaskCreatePinnedToCore};

pub struct JoinHandle<T>(mpsc::Receiver<T>);

impl<T> JoinHandle<T> {
    pub fn join(self) -> Result<T, impl Error> {
        return self.0.recv();
    }
}

pub fn spawn<F, T>(affinity: Core, code: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    /* TODO: What is good name, stack size and priority? Should they be given as parameters? */
    let name = CString::new("thread").unwrap();
    let stack_size = 6 * 1024;
    let priority = 1;

    let mut task_handle = ptr::null_mut();
    let (sender, receiver) = mpsc::sync_channel(1);

    /* C function which runs our closure and sends the return value back. */
    extern "C" fn code_runner<F, T>(param: *mut c_void)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (code, sender);
        unsafe {
            /* Should be safe because the pointer was made with Box::into_raw. */
            (code, sender) = *Box::from_raw(param as *mut (F, mpsc::SyncSender<T>));
        }

        /* Call the code and send return value back. */
        let return_value = code();
        let _ = sender.send(return_value);

        unsafe {
            /* FreeRTOS task must end with this. */
            vTaskDelete(ptr::null_mut());
        }
    }

    unsafe {
        /* Call the FreeRTOS task creation function. Code and sender are given as parameters to the
         * code runner. */
        let result = xTaskCreatePinnedToCore(
            Some(code_runner::<F, T>),
            name.as_ptr(),
            stack_size,
            Box::into_raw(Box::new((code, sender))) as *mut c_void,
            priority,
            &mut task_handle,
            affinity as i32,
        );
        if result != 1 {
            panic!("task creation failed: {}", result);
        }
    }

    /* Return join handle which waits on the receiver to receive the return value from our code. */
    return JoinHandle(receiver);
}
