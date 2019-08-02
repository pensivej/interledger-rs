#![recursion_limit = "128"]

use env_logger;
use futures::{Future, Stream};
use interledger::{
    cli,
    node::{AccountDetails, InterledgerNode},
};
use interledger_packet::Address;
use serde_json::json;
use std::str;
use std::str::FromStr;
use tokio::runtime::Builder as RuntimeBuilder;

mod redis_helpers;
use redis_helpers::*;

#[test]
fn three_nodes() {
    // Nodes 1 and 2 are peers, Node 2 is the parent of Node 3
    let _ = env_logger::try_init();
    let context = TestContext::new();

    // Each node will use its own DB within the redis instance
    let mut connection_info1 = context.get_client_connection_info();
    connection_info1.db = 1;
    let mut connection_info2 = context.get_client_connection_info();
    connection_info2.db = 2;
    let mut connection_info3 = context.get_client_connection_info();
    connection_info3.db = 3;

    let node1_http = get_open_port(Some(3010));
    let node1_settlement = get_open_port(Some(3011));
    let node2_http = get_open_port(Some(3020));
    let node2_settlement = get_open_port(Some(3021));
    let node2_btp = get_open_port(Some(3022));
    let node3_http = get_open_port(Some(3030));
    let node3_settlement = get_open_port(Some(3031));

    let mut runtime = RuntimeBuilder::new()
        .panic_handler(|_| panic!("Tokio worker panicked"))
        .build()
        .unwrap();

    let node1 = InterledgerNode {
        ilp_address: Address::from_str("example.one").unwrap(),
        default_spsp_account: Some(0),
        admin_auth_token: "admin".to_string(),
        redis_connection: connection_info1,
        btp_address: ([127, 0, 0, 1], get_open_port(None)).into(),
        http_address: ([127, 0, 0, 1], node1_http).into(),
        settlement_address: ([127, 0, 0, 1], node1_settlement).into(),
        secret_seed: cli::random_secret(),
        route_broadcast_interval: Some(200),
    };
    let node1_clone = node1.clone();
    runtime.spawn(
        // TODO insert the accounts via HTTP request
        node1_clone
            .insert_account(AccountDetails {
                ilp_address: Address::from_str("example.one").unwrap(),
                asset_code: "XYZ".to_string(),
                asset_scale: 9,
                btp_incoming_token: None,
                btp_uri: None,
                http_endpoint: None,
                http_incoming_token: Some("default account holder".to_string()),
                http_outgoing_token: None,
                max_packet_amount: u64::max_value(),
                min_balance: None,
                settle_threshold: None,
                settle_to: None,
                send_routes: false,
                receive_routes: false,
                routing_relation: None,
                round_trip_time: None,
                packets_per_minute_limit: None,
                amount_per_minute_limit: None,
                settlement_engine_url: None,
                settlement_engine_asset_scale: None,
            })
            .and_then(move |_|
        // TODO insert the accounts via HTTP request
        node1_clone
            .insert_account(AccountDetails {
                ilp_address: Address::from_str("example.two").unwrap(),
                asset_code: "XYZ".to_string(),
                asset_scale: 9,
                btp_incoming_token: None,
                btp_uri: None,
                http_endpoint: Some(format!("http://localhost:{}/ilp", node2_http)),
                http_incoming_token: Some("two".to_string()),
                http_outgoing_token: Some("one".to_string()),
                max_packet_amount: u64::max_value(),
                min_balance: Some(-1_000_000_000),
                settle_threshold: None,
                settle_to: None,
                send_routes: true,
                receive_routes: true,
                routing_relation: Some("Peer".to_string()),
                round_trip_time: None,
                packets_per_minute_limit: None,
                amount_per_minute_limit: None,
                settlement_engine_url: None,
                settlement_engine_asset_scale: None,
            }))
            .and_then(move |_| node1.serve()),
    );

    let node2 = InterledgerNode {
        ilp_address: Address::from_str("example.two").unwrap(),
        default_spsp_account: Some(0),
        admin_auth_token: "admin".to_string(),
        redis_connection: connection_info2,
        btp_address: ([127, 0, 0, 1], node2_btp).into(),
        http_address: ([127, 0, 0, 1], node2_http).into(),
        settlement_address: ([127, 0, 0, 1], node2_settlement).into(),
        secret_seed: cli::random_secret(),
        route_broadcast_interval: Some(200),
    };
    let node2_clone = node2.clone();
    runtime.spawn(
        node2_clone
            .insert_account(AccountDetails {
                ilp_address: Address::from_str("example.one").unwrap(),
                asset_code: "XYZ".to_string(),
                asset_scale: 9,
                btp_incoming_token: None,
                btp_uri: None,
                http_endpoint: Some(format!("http://localhost:{}/ilp", node1_http)),
                http_incoming_token: Some("one".to_string()),
                http_outgoing_token: Some("two".to_string()),
                max_packet_amount: u64::max_value(),
                min_balance: None,
                settle_threshold: None,
                settle_to: None,
                send_routes: true,
                receive_routes: true,
                routing_relation: Some("Peer".to_string()),
                round_trip_time: None,
                packets_per_minute_limit: None,
                amount_per_minute_limit: None,
                settlement_engine_url: None,
                settlement_engine_asset_scale: None,
            })
            .and_then(move |_| {
                node2_clone.insert_account(AccountDetails {
                    ilp_address: Address::from_str("example.two.three").unwrap(),
                    asset_code: "ABC".to_string(),
                    asset_scale: 6,
                    btp_incoming_token: Some("three".to_string()),
                    btp_uri: None,
                    http_endpoint: None,
                    http_incoming_token: Some("three".to_string()),
                    http_outgoing_token: None,
                    max_packet_amount: u64::max_value(),
                    min_balance: Some(-1_000_000_000),
                    settle_threshold: None,
                    settle_to: None,
                    send_routes: true,
                    receive_routes: false,
                    routing_relation: Some("Child".to_string()),
                    round_trip_time: None,
                    packets_per_minute_limit: None,
                    amount_per_minute_limit: None,
                    settlement_engine_url: None,
                    settlement_engine_asset_scale: None,
                })
            })
            .and_then(move |_| node2.serve())
            .and_then(move |_| {
                let client = reqwest::r#async::Client::new();
                client
                    .put(&format!("http://localhost:{}/rates", node2_http))
                    .header("Authorization", "Bearer admin")
                    .json(&json!({"ABC": 2, "XYZ": 1}))
                    .send()
                    .map_err(|err| panic!(err))
                    .and_then(|res| {
                        res.error_for_status()
                            .expect("Error setting exchange rates");
                        Ok(())
                    })
            }),
    );

    let node3 = InterledgerNode {
        ilp_address: Address::from_str("example.two.three").unwrap(),
        default_spsp_account: Some(0),
        admin_auth_token: "admin".to_string(),
        redis_connection: connection_info3,
        btp_address: ([127, 0, 0, 1], get_open_port(None)).into(),
        http_address: ([127, 0, 0, 1], node3_http).into(),
        settlement_address: ([127, 0, 0, 1], node3_settlement).into(),
        secret_seed: cli::random_secret(),
        route_broadcast_interval: Some(200),
    };
    let node3_clone = node3.clone();
    runtime.spawn(
        // Wait a bit to make sure the other node's BTP server is listening
        delay(50).map_err(|err| panic!(err)).and_then(move |_| {
            node3_clone
                .insert_account(AccountDetails {
                    ilp_address: Address::from_str("example.two.three").unwrap(),
                    asset_code: "ABC".to_string(),
                    asset_scale: 6,
                    btp_incoming_token: None,
                    btp_uri: None,
                    http_endpoint: None,
                    http_incoming_token: Some("default account holder".to_string()),
                    http_outgoing_token: None,
                    max_packet_amount: u64::max_value(),
                    min_balance: None,
                    settle_threshold: None,
                    settle_to: None,
                    send_routes: false,
                    receive_routes: false,
                    routing_relation: None,
                    round_trip_time: None,
                    packets_per_minute_limit: None,
                    amount_per_minute_limit: None,
                    settlement_engine_url: None,
                    settlement_engine_asset_scale: None,
                })
                .and_then(move |_| {
                    node3_clone.insert_account(AccountDetails {
                        ilp_address: Address::from_str("example.two").unwrap(),
                        asset_code: "ABC".to_string(),
                        asset_scale: 6,
                        btp_incoming_token: None,
                        btp_uri: Some(format!("btp+ws://:three@localhost:{}", node2_btp)),
                        http_endpoint: None,
                        http_incoming_token: None,
                        http_outgoing_token: None,
                        max_packet_amount: u64::max_value(),
                        min_balance: Some(-1_000_000_000),
                        settle_threshold: None,
                        settle_to: None,
                        send_routes: false,
                        receive_routes: true,
                        routing_relation: Some("Parent".to_string()),
                        round_trip_time: None,
                        packets_per_minute_limit: None,
                        amount_per_minute_limit: None,
                        settlement_engine_url: None,
                        settlement_engine_asset_scale: None,
                    })
                })
                .and_then(move |_| node3.serve())
        }),
    );

    runtime
        .block_on(
            // Wait for the nodes to spin up
            delay(100)
                .map_err(|_| panic!("Something strange happened"))
                .and_then(move |_| {
                    let client = reqwest::r#async::Client::new();
                    let send_1_to_3 = client
                        .post(&format!("http://localhost:{}/pay", node1_http))
                        .header("Authorization", "Bearer default account holder")
                        .json(&json!({
                            "receiver": format!("http://localhost:{}/.well-known/pay", node3_http),
                            "source_amount": 1000,
                        }))
                        .send()
                        .and_then(|res| res.error_for_status())
                    .and_then(|res| res.into_body().concat2())
                    .and_then(|body| {
                        assert_eq!(str::from_utf8(body.as_ref()).unwrap(), "{\"delivered_amount\":2}");
                        Ok(())
                    });

                    let send_3_to_1 = client
                        .post(&format!("http://localhost:{}/pay", node3_http))
                        .header("Authorization", "Bearer default account holder")
                        .json(&json!({
                                "receiver": format!("http://localhost:{}/.well-known/pay", node1_http).as_str(),
                            "source_amount": 1000,
                        }))
                        .send()
                        .and_then(|res| res.error_for_status())
                    .and_then(|res| res.into_body().concat2())
                    .and_then(|body| {
                        assert_eq!(str::from_utf8(body.as_ref()).unwrap(), "{\"delivered_amount\":500000}");
                        Ok(())
                    });

                    let get_balance = |account_id, node_port, admin_token| {
                        let client = reqwest::r#async::Client::new();
                        client
                            .get(&format!(
                                "http://localhost:{}/accounts/{}/balance",
                                node_port, account_id
                            ))
                            .header("Authorization", format!("Bearer {}", admin_token))
                            .send()
                            .map_err(|err| {
                                eprintln!("Error getting account data: {:?}", err);
                                err
                            })
                            .and_then(|res| res.error_for_status())
                            .and_then(|res| res.into_body().concat2())
                    };

                    // Node 1 sends 1000 to Node 3. However, Node1's scale is 9,
                    // while Node 3's scale is 6. This means that Node 3 will
                    // see 1000x less. In addition, the conversion rate is 2:1
                    // for 3's asset, so he will receive 2 total.
                    send_1_to_3
                        .map_err(|err| {
                            eprintln!("Error sending from node 1 to node 3: {:?}", err);
                            err
                        })
                        .and_then(move |_| {
                            get_balance(0, node1_http, "default account holder")
                            .and_then(move |ret| {
                                let ret = str::from_utf8(&ret).unwrap();
                                assert_eq!(ret, "{\"balance\":\"-1000\"}");
                                Ok(())
                            }).and_then(move |_| {
                                // Node 2 updates Node 3's balance properly.
                                get_balance(1, node2_http, "three").and_then(move |ret| {
                                    let ret = str::from_utf8(&ret).unwrap();
                                    assert_eq!(ret, "{\"balance\":\"2\"}");
                                    Ok(())
                                })
                            }).and_then(move |_| {
                                // Node 3's balance is properly adjusted after
                                // it's received the message from node 2
                                get_balance(0, node3_http, "default account holder").and_then(move |ret| {
                                    let ret = str::from_utf8(&ret).unwrap();
                                    assert_eq!(ret, "{\"balance\":\"2\"}");
                                    Ok(())
                                })
                            })
                        })
                        .and_then(move |_| send_3_to_1
                        .map_err(|err| {
                            eprintln!("Error sending from node 3 to node 1: {:?}", err);
                            err
                        }))
                        .and_then(move |_| {
                            get_balance(0, node1_http, "default account holder").and_then(move |ret| {
                                let ret = str::from_utf8(&ret).unwrap();
                                assert_eq!(ret, "{\"balance\":\"499000\"}");
                                Ok(())
                            }).and_then(move |_| {
                                // Node 2 updates Node 3's balance properly.
                                get_balance(1, node2_http, "three").and_then(move |ret| {
                                    let ret = str::from_utf8(&ret).unwrap();
                                    assert_eq!(ret, "{\"balance\":\"-998\"}");
                                    Ok(())
                                })
                            }).and_then(move |_| {
                                // Node 3's balance is properly adjusted after
                                // it's received the message from node 2
                                get_balance(0, node3_http, "default account holder").and_then(move |ret| {
                                    let ret = str::from_utf8(&ret).unwrap();
                                    assert_eq!(ret, "{\"balance\":\"-998\"}");
                                    Ok(())
                                })
                            })
                        })
                }),
        )
        .unwrap();
}
