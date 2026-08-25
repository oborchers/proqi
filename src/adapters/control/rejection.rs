use crate::ports::control::{ControlRejectionCode, ControlRequest, ControlResponse, ControlResult};

pub(super) fn rejected(
    request: &ControlRequest,
    code: ControlRejectionCode,
    message: &str,
) -> ControlResponse {
    ControlResponse {
        protocol: request.protocol,
        request_id: request.request_id,
        result: ControlResult::Rejected {
            code: code.as_str().to_owned(),
            message: message.to_owned(),
        },
    }
}
