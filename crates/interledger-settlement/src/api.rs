use super::{SettlementAccount, SettlementStore};
use futures::{
    future::{ok, result, Either},
    Future,
};
use hyper::Response;
use interledger_ildcp::IldcpAccount;
use interledger_packet::PrepareBuilder;
use interledger_service::{AccountStore, OutgoingRequest, OutgoingService};
use interledger_service_util::Convert;
use std::{
    marker::PhantomData,
    str::{self, FromStr},
    time::{Duration, SystemTime},
};

static PEER_PROTOCOL_CONDITION: [u8; 32] = [
    102, 104, 122, 173, 248, 98, 189, 119, 108, 143, 193, 139, 142, 159, 142, 32, 8, 151, 20, 133,
    110, 226, 51, 179, 144, 42, 89, 29, 13, 95, 41, 37,
];

pub struct SettlementApi<S, O, A> {
    outgoing_handler: O,
    store: S,
    account_type: PhantomData<A>,
}

#[derive(Debug, Response)]
#[web(status = "200")]
struct Success;

// TODO add authentication

impl_web! {
    impl<S, O, A> SettlementApi<S, O, A>
    where
        S: SettlementStore<Account = A> + AccountStore<Account = A> + Clone + Send + Sync + 'static,
        O: OutgoingService<A> + Clone + Send + Sync + 'static,
        A: SettlementAccount + IldcpAccount + Send + Sync + 'static,
    {
        pub fn new(store: S, outgoing_handler: O) -> Self {
            SettlementApi {
                store,
                outgoing_handler,
                account_type: PhantomData,
            }
        }


        // TODO: Can the Response<()> be converted to a Response<String>? It'd
        // be nice if we could include the full error message body (currently
        // it's just the header)
        // TODO: Verify that the idempotency_key is seen as a "Idempotency-Key"
        // header by impl_web!
        #[post("/accounts/:account_id/settlement")]
        fn receive_settlement(&self, account_id: String, body: u64, idempotency_key: String) -> impl Future<Item = Success, Error = Response<()>> {
            let amount = body;
            let store = self.store.clone();
            let mut store_clone = store.clone();
            result(A::AccountId::from_str(&account_id)
                .map_err(move |_err| {
                    error!("Unable to parse account id: {}", account_id);
                    Response::builder().status(400).body(()).unwrap()
                }))
                .and_then(move |account_id| store.get_accounts(vec![account_id]).map_err(move |_| {
                    error!("Error getting account: {}", account_id);
                    Response::builder().status(404).body(()).unwrap()
                }))
                .and_then(|accounts| {
                    let account = &accounts[0];
                    if let Some(settlement_engine) = account.settlement_engine_details() {
                        Ok((account.clone(), settlement_engine))
                    } else {
                        error!("Account {} does not have settlement engine details configured. Cannot handle incoming settlement", account.id());
                        Err(Response::builder().status(404).body(()).unwrap())
                    }
                })
                .and_then(move |(account, settlement_engine)| {
                    let account_id = account.id();
                    let amount = amount.normalize_scale(account.asset_scale(), settlement_engine.asset_scale);

                    store_clone.update_balance_for_incoming_settlement(account_id, amount, idempotency_key)
                        .map_err(move |_| {
                            error!("Error updating balance of account: {} for incoming settlement of amount: {}", account_id, amount);
                            Response::builder().status(500).body(()).unwrap()
                        })
                })
                .and_then(|_| Ok(Success))
        }

        // Gets called by our settlement engine, forwards the request outwards
        // until it reaches the peer's settlement engine
        #[post("/accounts/:account_id/messages")]
        fn send_outgoing_message(&self, account_id: String, body: Vec<u8>, idempotency_key: String)-> impl Future<Item = Vec<u8>, Error = Response<()>> {
            let mut store: S = self.store.clone();
            let mut store_clone = self.store.clone(); // TODO: Can we avoid these clones?
            let mut outgoing_handler = self.outgoing_handler.clone();

            // Check store for idempotency key. If exists, return cached data
            store.load_idempotent_data(idempotency_key.clone())
            .map_err(move |err| {
                error!("Couldn't connect to store {:?}", err);
                Response::builder().status(500).body(()).unwrap()
            }).and_then(move |data: Option<Vec<u8>>| {
                if let Some(d) = data {
                    return Either::A(ok(d))
                }

                Either::B(
                result(A::AccountId::from_str(&account_id)
                .map_err(move |_err| {
                    error!("Unable to parse account id: {}", account_id);
                    Response::builder().status(400).body(()).unwrap()
                }))
                .and_then(move |account_id| store.get_accounts(vec![account_id]).map_err(move |_| {
                    error!("Error getting account: {}", account_id);
                    Response::builder().status(404).body(()).unwrap()
                }))
                .and_then(|accounts| {
                    let account = &accounts[0];
                    if let Some(settlement_engine) = account.settlement_engine_details() {
                        Ok((account.clone(), settlement_engine))
                    } else {
                        error!("Account {} has no settlement engine details configured, cannot send a settlement engine message to that account", accounts[0].id());
                        Err(Response::builder().status(404).body(()).unwrap())
                    }
                })
                .and_then(move |(account, settlement_engine)| {
                    // Send the message to the peer's settlement engine.
                    // Note that we use dummy values for the `from` and `original_amount`
                    // because this `OutgoingRequest` will bypass the router and thus will not
                    // use either of these values. Including dummy values in the rare case where
                    // we do not need them seems easier than using
                    // `Option`s all over the place.
                    outgoing_handler.send_request(OutgoingRequest {
                        from: account.clone(),
                        to: account.clone(),
                        original_amount: 0,
                        prepare: PrepareBuilder {
                            destination: settlement_engine.ilp_address,
                            amount: 0,
                            expires_at: SystemTime::now() + Duration::from_secs(30),
                            data: &body,
                            execution_condition: &PEER_PROTOCOL_CONDITION,
                        }.build()
                    })
                    .map_err(|reject| {
                        error!("Error sending message to peer settlement engine. Packet rejected with code: {}, message: {}", reject.code(), str::from_utf8(reject.message()).unwrap_or_default());
                        Response::builder().status(500).body(()).unwrap()
                    })
                })
                .and_then(move |fulfill| {
                    let data = fulfill.data().to_vec();
                    // Should this be and_then or spawn'ed?
                    // Save the idempotency key and the data such that
                    // the same response is returned
                    store_clone.save_idempotent_data(idempotency_key, data.clone());
                    ok(data)
                }))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use crate::test_helpers::*;

    // Settlement Tests

    mod settlement_tests {
        use super::*;

        #[test]
        fn settlement_ok() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = test_store(false, true);
            let api = test_api(store, false);

            let ret = api
                .receive_settlement(id, SETTLEMENT_BODY, IDEMPOTENCY.to_string())
                .wait();
            assert!(ret.is_ok());
        }

        #[test]
        fn account_has_no_engine_configured() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = test_store(false, false);
            let api = test_api(store, false);

            let ret = api
                .receive_settlement(id, SETTLEMENT_BODY, IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 404);
        }

        #[test]
        fn update_balance_for_incoming_settlement_fails() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = test_store(true, true);
            let api = test_api(store, false);

            let ret: Response<_> = api
                .receive_settlement(id, SETTLEMENT_BODY, IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 500);
        }

        #[test]
        fn invalid_account_id() {
            // the api is configured to take an accountId type
            // supplying an id that cannot be parsed to that type must fail
            let id = "a".to_string();
            let store = test_store(false, true);
            let api = test_api(store, false);

            let ret: Response<_> = api
                .receive_settlement(id, SETTLEMENT_BODY, IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 400);
        }

        #[test]
        fn account_not_in_store() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = TestStore::new(vec![], false);
            let api = test_api(store, false);

            let ret: Response<_> = api
                .receive_settlement(id, SETTLEMENT_BODY, IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 404);
        }
    }

    mod message_tests {
        use super::*;

        #[test]
        fn message_ok() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = test_store(false, true);
            let api = test_api(store, true);

            let ret = api
                .send_outgoing_message(id, vec![], IDEMPOTENCY.to_string())
                .wait()
                .unwrap();
            assert_eq!(ret, b"hello!");
        }

        #[test]
        fn message_gets_rejected() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = test_store(false, true);
            let api = test_api(store, false);

            let ret = api
                .send_outgoing_message(id, vec![], IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 500);
        }

        #[test]
        fn invalid_account_id() {
            // the api is configured to take an accountId type
            // supplying an id that cannot be parsed to that type must fail
            let id = "a".to_string();
            let store = test_store(false, true);
            let api = test_api(store, true);

            let ret: Response<_> = api
                .send_outgoing_message(id, vec![], IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 400);
        }

        #[test]
        fn account_not_in_store() {
            let id = TEST_ACCOUNT_0.clone().id.to_string();
            let store = TestStore::new(vec![], false);
            let api = test_api(store, true);

            let ret: Response<_> = api
                .send_outgoing_message(id, vec![], IDEMPOTENCY.to_string())
                .wait()
                .unwrap_err();
            assert_eq!(ret.status().as_u16(), 404);
        }

    }
}
