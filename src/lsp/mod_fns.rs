use super::*;

#[must_use]
pub fn route_response_pub(method: &str, uri: Option<&str>, result: Value) -> Option<LspMessage> {
    route_response(method, uri, result)
}

#[must_use]
pub fn route_notification_pub(method: &str, params: &Value) -> Option<LspMessage> {
    route_notification(method, params)
}

#[must_use]
pub fn extract_hover_text_pub(result: &Value) -> Option<String> {
    extract_hover_text(result)
}
