//! Opening links and, later, other resources in the program the operating
//! system picks for them.
//!
//! Only values the system already checked reach this module, so what it
//! hands to the operating system is limited by their types.

use crate::system::ExternalUrl;

/// Opens `url` in the default application for its scheme, without waiting
/// for that application. A failure, such as a machine with no web browser
/// or no graphical session, is logged and does not affect the session.
pub(crate) fn open_url(url: &ExternalUrl) {
    match open::that_detached(url.as_str()) {
        Ok(()) => tracing::info!(url = url.as_str(), "opened an external link"),
        Err(err) => tracing::warn!(
            url = url.as_str(),
            error = %err,
            "failed to open an external link"
        ),
    }
}
