use interledger_api::AccountDetails;
use interledger_packet::Address;
use lazy_static::lazy_static;
use std::str::FromStr;

lazy_static! {
    pub static ref ACCOUNT_DETAILS_0: AccountDetails = AccountDetails {
        ilp_address: Address::from_str("example.alice").unwrap(),
        asset_scale: 6,
        asset_code: "XYZ".to_string(),
        max_packet_amount: 1000,
        min_balance: Some(-1000),
        http_endpoint: Some("http://example.com/ilp".to_string()),
        http_incoming_token: Some("incoming_auth_token".to_string()),
        http_outgoing_token: Some("outgoing_auth_token".to_string()),
        btp_uri: Some("btp+ws://:btp_token@example.com/btp".to_string()),
        btp_incoming_token: Some("btp_token".to_string()),
        settle_threshold: Some(0),
        settle_to: Some(-1000),
        send_routes: false,
        receive_routes: true,
        routing_relation: None,
        round_trip_time: None,
        amount_per_minute_limit: Some(1000),
        packets_per_minute_limit: Some(2),
        settlement_engine_url: None,
        settlement_engine_asset_scale: None,
    };
    pub static ref ACCOUNT_DETAILS_1: AccountDetails = AccountDetails {
        ilp_address: Address::from_str("example.bob").unwrap(),
        asset_scale: 9,
        asset_code: "ABC".to_string(),
        max_packet_amount: 1_000_000,
        min_balance: Some(0),
        http_endpoint: Some("http://example.com/ilp".to_string()),
        http_incoming_token: Some("QWxhZGRpbjpPcGVuU2VzYW1l".to_string()),
        http_outgoing_token: Some("outgoing_auth_token".to_string()),
        btp_uri: Some("btp+ws://:other_outgoing_btp_token@example.com/btp".to_string()),
        btp_incoming_token: Some("other_btp_token".to_string()),
        settle_threshold: Some(0),
        settle_to: Some(-1000),
        send_routes: true,
        receive_routes: false,
        routing_relation: None,
        round_trip_time: None,
        amount_per_minute_limit: Some(1000),
        packets_per_minute_limit: Some(20),
        settlement_engine_url: None,
        settlement_engine_asset_scale: None,
    };
    pub static ref ACCOUNT_DETAILS_2: AccountDetails = AccountDetails {
        ilp_address: Address::from_str("example.charlie").unwrap(),
        asset_scale: 9,
        asset_code: "XRP".to_string(),
        max_packet_amount: 1000,
        min_balance: Some(0),
        http_endpoint: None,
        http_incoming_token: None,
        http_outgoing_token: None,
        btp_uri: None,
        btp_incoming_token: None,
        settle_threshold: Some(0),
        settle_to: None,
        send_routes: false,
        receive_routes: false,
        routing_relation: None,
        round_trip_time: None,
        amount_per_minute_limit: None,
        packets_per_minute_limit: None,
        settlement_engine_url: None,
        settlement_engine_asset_scale: None,
    };
}
