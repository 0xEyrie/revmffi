use crate::memory::UnmanagedVector;

use super::BackendError;

/// This enum gives names to the status codes returned from Go callbacks to Rust.
/// The Go code will return one of these variants when returning.
///
/// 0 means no error, all the other cases are some sort of error.
///
/// cbindgen:prefix-with-name
// NOTE TO DEVS: If you change the values assigned to the variants of this enum, You must also
//               update the match statement in the From conversion below.
//               Otherwise all hell may break loose.
//               You have been warned.
#[repr(i32)] // This makes it so the enum looks like a simple i32 to Go
#[derive(PartialEq)]
pub enum GoError {
    None = 0,
    /// Go panicked for an unexpected reason.
    Panic = 1,
    /// Go received a bad argument from Rust
    BadArgument = 2,
    /// Error while trying to serialize data in Go code (typically json.Marshal)
    CannotSerialize = 3,
    /// An error happened during normal operation of a Go callback, which should be fed back to the
    /// contract
    User = 4,
    /// Unimplemented
    Unimplemented = 5,
    /// An error type that should never be created by us. It only serves as a fallback for the i32
    /// to GoError conversion.
    Other = -1,
}

impl From<i32> for GoError {
    fn from(n: i32) -> Self {
        // This conversion treats any number that is not otherwise an expected value as
        // `GoError::Other`
        match n {
            0 => GoError::None,
            1 => GoError::Panic,
            2 => GoError::BadArgument,
            4 => GoError::CannotSerialize,
            5 => GoError::User,
            6 => GoError::Unimplemented,
            _ => GoError::Other,
        }
    }
}

impl GoError {
    /// This converts a GoError to a `Result<(), BackendError>`, using a fallback error message for
    /// some cases. If it is GoError::User the error message will be returned to the contract.
    /// Otherwise, the returned error will trigger a trap in the VM and abort contract execution
    /// immediately.
    ///
    /// This reads data from an externally provided `UnmanagedVector` and assumes UFT-8 encoding. To
    /// protect against invalid UTF-8 data, a lossy conversion to string is used. The data is
    /// limited to 8KB in order to protect against long externally generated error messages.
    ///
    /// The `error_msg` is always consumed here and must not be used afterwards.
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn into_result<F>(
        self,
        error_msg: UnmanagedVector,
        default_error_msg: F,
    ) -> Result<(), BackendError>
    where
        F: FnOnce() -> String,
    {
        const MAX_ERROR_LEN: usize = 8 * 1024;

        // We destruct the UnmanagedVector here, no matter if we need the data.
        let error_msg = error_msg.consume();

        let build_error_msg = || -> String {
            match error_msg {
                Some(mut data) => {
                    data.truncate(MAX_ERROR_LEN);
                    String::from_utf8_lossy(&data).into()
                }
                None => default_error_msg(),
            }
        };

        match self {
            // Success
            GoError::None => Ok(()),
            // Errors with direct counterpart
            GoError::Panic => Err(BackendError::foreign_panic()),
            GoError::BadArgument => Err(BackendError::bad_argument()),
            GoError::User => Err(BackendError::user_err(build_error_msg())),
            GoError::Unimplemented => Err(BackendError::unimplemented()),
            // Everything else goes into unknown
            GoError::CannotSerialize | GoError::Other => {
                Err(BackendError::unknown(build_error_msg()))
            }
        }
    }
}
