//! Compatibility shakedown for the pinned vss-client-ng v0.5.0 dependency against current
//! vss-server master. This test assumes a no-auth VSS server is already running at
//! `localhost:8080` and exercises a full client lifecycle through the public v0.5.0 client API:
//! empty listing, missing-key reads, conditional and non-conditional writes, gets, conflict
//! handling, transactional put/delete, direct deletes, paginated listing, and cleanup.

#![cfg(vss_client_v050_compatibility)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vss_client_v050::client::VssClient;
use vss_client_v050::error::VssError;
use vss_client_v050::types::{
	DeleteObjectRequest, GetObjectRequest, GetObjectResponse, KeyValue, ListKeyVersionsRequest,
	PutObjectRequest,
};
use vss_client_v050::util::retry::{ExponentialBackoffRetryPolicy, RetryPolicy};

const VSS_SERVER_BASE_URL: &str = "http://localhost:8080/vss";
const KEY_ALPHA: &str = "compat/alpha";
const KEY_BETA: &str = "compat/beta";
const KEY_DELTA: &str = "compat/delta";
const KEY_EPSILON: &str = "compat/epsilon";
const KEY_GAMMA: &str = "compat/gamma";
const KEY_OUTSIDE_PREFIX: &str = "outside-prefix";
const KEY_STALE_GLOBAL: &str = "compat/stale-global";
const KEY_THETA: &str = "compat/theta";
const KEY_PREFIX: &str = "compat/";
const GLOBAL_VERSION_KEY: &str = "global_version";
const LIST_PAGE_SIZE: i32 = 2;

#[tokio::test]
async fn test_vss_client_v050_compatibility() -> Result<(), VssError> {
	let client = VssClient::new(VSS_SERVER_BASE_URL.to_string(), retry_policy());
	let store_id = unique_store_id();
	let mut global_version = 0;

	let empty_list =
		client.list_key_versions(&list_request(&store_id, None, Some(10), None)).await?;
	// A new store should report the initial global version.
	assert_eq!(empty_list.global_version, Some(global_version));
	// A new store should not contain any key-version entries.
	assert!(empty_list.key_versions.is_empty());
	// An empty result set should also be the final page.
	assert_eq!(empty_list.next_page_token.as_deref(), Some(""));

	// Reading a key that has never been written should surface the protocol's missing-key error.
	assert_no_such_key(client.get_object(&get_request(&store_id, "missing")).await, "missing");

	client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![kv(KEY_ALPHA, 0, b"alpha-v1"), kv(KEY_BETA, 0, b"beta-v1")],
			vec![],
		))
		.await?;
	global_version += 1;

	// The first conditional write should make alpha readable at server-side version 1.
	assert_key_value(&client, &store_id, KEY_ALPHA, 1, b"alpha-v1").await?;
	// The first conditional write should make beta readable at server-side version 1.
	assert_key_value(&client, &store_id, KEY_BETA, 1, b"beta-v1").await?;

	client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![
				kv(KEY_ALPHA, 1, b"alpha-v2"),
				kv(KEY_GAMMA, 0, b"gamma-v1"),
				kv(KEY_OUTSIDE_PREFIX, 0, b"outside-prefix-v1"),
			],
			vec![],
		))
		.await?;
	global_version += 1;

	// Updating alpha with the matching key version should advance alpha to version 2.
	assert_key_value(&client, &store_id, KEY_ALPHA, 2, b"alpha-v2").await?;
	// Creating gamma in the same request should make it readable at version 1.
	assert_key_value(&client, &store_id, KEY_GAMMA, 1, b"gamma-v1").await?;

	let stale_put = client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![kv(KEY_ALPHA, 1, b"stale-alpha")],
			vec![],
		))
		.await;
	// Reusing alpha's old key version should be rejected as a conflict.
	assert_conflict(stale_put);
	// The rejected stale write must not change alpha's committed value.
	assert_key_value(&client, &store_id, KEY_ALPHA, 2, b"alpha-v2").await?;

	let stale_global_version_put = client
		.put_object(&put_request(
			&store_id,
			Some(global_version - 1),
			vec![kv(KEY_STALE_GLOBAL, 0, b"stale-global-version")],
			vec![],
		))
		.await;
	// Reusing an old global version should be rejected independently of key-level versions.
	assert_conflict(stale_global_version_put);
	// A failed global-version write must not create the requested key.
	assert_no_such_key(
		client.get_object(&get_request(&store_id, KEY_STALE_GLOBAL)).await,
		KEY_STALE_GLOBAL,
	);

	client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![kv(KEY_DELTA, -1, b"delta-v1")],
			vec![],
		))
		.await?;
	global_version += 1;

	client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![kv(KEY_DELTA, -1, b"delta-v2")],
			vec![],
		))
		.await?;
	global_version += 1;

	// Non-conditional writes should reset the server-side key version to 1 and keep the last value.
	assert_key_value(&client, &store_id, KEY_DELTA, 1, b"delta-v2").await?;

	client
		.put_object(&put_request(
			&store_id,
			Some(global_version),
			vec![kv(KEY_THETA, 0, b"theta-v1")],
			vec![kv(KEY_BETA, 1, b"")],
		))
		.await?;
	global_version += 1;

	// A transaction mixing a put and delete should commit the put side.
	assert_key_value(&client, &store_id, KEY_THETA, 1, b"theta-v1").await?;
	// The same transaction should remove beta atomically.
	assert_no_such_key(client.get_object(&get_request(&store_id, KEY_BETA)).await, KEY_BETA);

	client.delete_object(&delete_request(&store_id, KEY_GAMMA, 1)).await?;
	client.delete_object(&delete_request(&store_id, KEY_GAMMA, 1)).await?;
	// Repeating a direct delete should leave gamma deleted and exercise delete idempotency.
	assert_no_such_key(client.get_object(&get_request(&store_id, KEY_GAMMA)).await, KEY_GAMMA);

	client
		.put_object(&put_request(&store_id, None, vec![kv(KEY_EPSILON, 0, b"epsilon-v1")], vec![]))
		.await?;
	// A write without global-version checking should still create the key at version 1.
	assert_key_value(&client, &store_id, KEY_EPSILON, 1, b"epsilon-v1").await?;

	let listed_versions =
		list_all_key_versions(&client, &store_id, Some(KEY_PREFIX), global_version).await?;
	let listed_keys: BTreeSet<&str> = listed_versions.keys().map(String::as_str).collect();
	// Prefix listing should include only the live keys under compat/ after deletes and conflicts.
	assert_eq!(listed_keys, BTreeSet::from([KEY_ALPHA, KEY_DELTA, KEY_EPSILON, KEY_THETA]));
	// Listing should report alpha's latest key version.
	assert_eq!(listed_versions[KEY_ALPHA], 2);
	// Listing should report delta's non-conditional write version.
	assert_eq!(listed_versions[KEY_DELTA], 1);
	// Listing should report epsilon's no-global-version write version.
	assert_eq!(listed_versions[KEY_EPSILON], 1);
	// Listing should report theta's transactional write version.
	assert_eq!(listed_versions[KEY_THETA], 1);

	let cleanup_keys =
		[KEY_ALPHA, KEY_DELTA, KEY_EPSILON, KEY_THETA, KEY_OUTSIDE_PREFIX, GLOBAL_VERSION_KEY];
	for key in cleanup_keys {
		client.delete_object(&delete_request(&store_id, key, -1)).await?;
	}

	let final_list =
		client.list_key_versions(&list_request(&store_id, None, Some(10), None)).await?;
	// Deleting the protocol global-version key should make the store report the default version.
	assert_eq!(final_list.global_version, Some(0));
	// Cleanup should leave no key-version entries behind for this store.
	assert!(final_list.key_versions.is_empty());
	// Cleanup should leave the final list response on its last page.
	assert_eq!(final_list.next_page_token.as_deref(), Some(""));

	Ok(())
}

fn retry_policy() -> impl RetryPolicy<E = VssError> {
	ExponentialBackoffRetryPolicy::new(Duration::from_millis(10)).with_max_attempts(1)
}

fn unique_store_id() -> String {
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system clock must be after UNIX epoch")
		.as_nanos();
	format!("v050-compat-{nanos}")
}

fn get_request(store_id: &str, key: &str) -> GetObjectRequest {
	GetObjectRequest { store_id: store_id.to_string(), key: key.to_string() }
}

fn put_request(
	store_id: &str, global_version: Option<i64>, transaction_items: Vec<KeyValue>,
	delete_items: Vec<KeyValue>,
) -> PutObjectRequest {
	PutObjectRequest {
		store_id: store_id.to_string(),
		global_version,
		transaction_items,
		delete_items,
	}
}

fn delete_request(store_id: &str, key: &str, version: i64) -> DeleteObjectRequest {
	DeleteObjectRequest { store_id: store_id.to_string(), key_value: Some(kv(key, version, b"")) }
}

fn list_request(
	store_id: &str, page_token: Option<String>, page_size: Option<i32>, key_prefix: Option<&str>,
) -> ListKeyVersionsRequest {
	ListKeyVersionsRequest {
		store_id: store_id.to_string(),
		key_prefix: key_prefix.map(str::to_string),
		page_size,
		page_token,
	}
}

fn kv(key: &str, version: i64, value: &[u8]) -> KeyValue {
	KeyValue { key: key.to_string(), version, value: value.to_vec() }
}

async fn assert_key_value(
	client: &VssClient<impl RetryPolicy<E = VssError>>, store_id: &str, key: &str,
	expected_version: i64, expected_value: &[u8],
) -> Result<(), VssError> {
	let response = client.get_object(&get_request(store_id, key)).await?;
	let value = response_value(response, key);
	// The server must echo the requested key in a successful get response.
	assert_eq!(value.key, key);
	// The key-level version must match the lifecycle step's expected version.
	assert_eq!(value.version, expected_version);
	// The stored bytes must round-trip unchanged through the v0.5.0 client.
	assert_eq!(value.value, expected_value);
	Ok(())
}

fn response_value(response: GetObjectResponse, key: &str) -> KeyValue {
	// A successful get response must include a KeyValue payload.
	response.value.unwrap_or_else(|| panic!("expected GetObjectResponse to include {key}"))
}

fn assert_no_such_key(result: Result<GetObjectResponse, VssError>, key: &str) {
	match result {
		// The expected protocol error is the only accepted missing-key outcome.
		Err(VssError::NoSuchKeyError(_)) => {},
		// Any other error would indicate the request failed for the wrong reason.
		Err(e) => panic!("expected {key} to be missing, got {e}"),
		// A successful get would mean the key unexpectedly exists.
		Ok(_) => panic!("expected {key} to be missing"),
	}
}

fn assert_conflict<T>(result: Result<T, VssError>) {
	match result {
		// The expected protocol error is the only accepted conflict outcome.
		Err(VssError::ConflictError(_)) => {},
		// Any other error would indicate the rejected write failed for the wrong reason.
		Err(e) => panic!("expected conflict error, got {e}"),
		// A successful write would mean conflict detection is not working.
		Ok(_) => panic!("expected conflict error"),
	}
}

async fn list_all_key_versions(
	client: &VssClient<impl RetryPolicy<E = VssError>>, store_id: &str, key_prefix: Option<&str>,
	expected_global_version: i64,
) -> Result<BTreeMap<String, i64>, VssError> {
	let mut page_token = None;
	let mut key_versions = BTreeMap::new();
	let mut page_count = 0;

	loop {
		let page = client
			.list_key_versions(&list_request(
				store_id,
				page_token.take(),
				Some(LIST_PAGE_SIZE),
				key_prefix,
			))
			.await?;
		// Each paginated response must honor the requested maximum page size.
		assert!(page.key_versions.len() <= LIST_PAGE_SIZE as usize);

		if page_count == 0 {
			// Only the first page should include the store's global version.
			assert_eq!(page.global_version, Some(expected_global_version));
		} else {
			// Follow-up pages should omit the global version per the VSS protocol.
			assert!(page.global_version.is_none());
		}
		page_count += 1;

		for key_value in page.key_versions {
			// List responses should include only key/version metadata, not stored values.
			assert!(key_value.value.is_empty());
			key_versions.insert(key_value.key, key_value.version);
		}

		match page.next_page_token {
			Some(token) if !token.is_empty() => page_token = Some(token),
			_ => break,
		}
	}

	// With four matching keys and a page size of two, this path must exercise pagination.
	assert!(page_count > 1);
	Ok(key_versions)
}
