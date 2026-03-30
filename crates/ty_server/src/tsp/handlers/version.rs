//! Handler for `typeServer/getSupportedProtocolVersion`.

use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::tsp::protocol::PROTOCOL_VERSION;
use crate::tsp::requests::GetSupportedProtocolVersion;

pub(crate) struct GetSupportedProtocolVersionHandler;

impl RequestHandler for GetSupportedProtocolVersionHandler {
    type RequestType = GetSupportedProtocolVersion;
}

impl SyncRequestHandler for GetSupportedProtocolVersionHandler {
    fn run(_session: &mut Session, _client: &Client, _params: ()) -> crate::server::Result<String> {
        Ok(PROTOCOL_VERSION.to_string())
    }
}
