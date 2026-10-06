# Changelog

## 0.5.0 (2026-10-06)

This release contains breaking changes. See **Upgrading from 0.4** below.

### Breaking changes

- `JsonMC::invoke` takes only the request: `c.invoke(&request)`. The service and
  method are read from `request.service` and `request.method`, so they can no
  longer disagree with the request body.
- `Xmlmc::new` and `JsonMC::new` return `Result<_, ApiError>` instead of
  `Result<_, Box<dyn Error>>`, and return `ApiError::InvalidUrl` if the server
  url cannot be parsed.
- `set_param`, `set_param_attr`, `open_element` and `close_element` return
  `Result<(), ApiError>` (with `ApiError::InvalidXml`) instead of
  `Result<(), &str>`.
- `set_timeout` and `set_user_agent` return `Result<(), ApiError>`. An invalid
  user agent is now reported instead of being silently ignored, and the
  previous setting is kept.
- `get_session_id` returns the bare session id rather than the
  `ESPSessionState=<id>` cookie. `set_sessionid` accepts either form.
- `get_headers` returns `&HeaderMap` instead of a cloned `HeaderMap`.
- The xmlmc trace attribute is sent as `rustApi/<trace>` instead of
  `goApi/<trace>`.
- The default user agent is `rust_apilib/<crate version>` instead of
  `rust_apilib/1.1`.
- `ApiError` has new variants: `InvalidXml`, `Api` and `InvalidUrl`.
  Exhaustive matches on `ApiError` need updating.

### Added

- `JsonMC` now has `set_sessionid`, `set_copy_headers`, `get_headers`,
  `get_status_code`, `get_server_url` and `get_count`, matching `Xmlmc`.
- `Response::into_result()` returns `Ok(params)` or `Err(ResponseState)`.
  `ResponseState` implements `Error`, and `?` converts it into `ApiError::Api`.
- `Attributes::new(key, value)`, so `set_param_attr` can be used outside the
  crate.

### Fixed

- `get_url_from_name` now fails over to the backup fileserver when the primary
  can't be reached at all (not just on a non-200 response), and the lookup has
  a 10 second timeout.
- The `service`, `method` and `trace` values are xml encoded in the
  `<methodCall>` header.
- Service and method names are percent-encoded in the request url.
- Server urls ending in `/` (as returned by `get_url_from_name`) no longer
  produce `//` in request urls.
- No `Cookie` header is sent when there is no session.
- `Xmlmc` clears its params on every `invoke`, including when the request
  fails to send.

### Changed

- `Xmlmc` and `JsonMC` share their http handling internally.
- The `base64` dependency is now a dev-dependency (only the examples use it),
  and the `regex` dependency has been removed.
- The crate uses the 2021 edition.

### Upgrading from 0.4

```rust
// 0.4
let response: Response = c.invoke("session", "userLogon", &request).await?;
c.set_timeout(60);
c.set_user_agent("my_tool/1.0");
let headers = c.get_headers();

// 0.5
let response: Response = c.invoke(&request).await?;
c.set_timeout(60)?;
c.set_user_agent("my_tool/1.0")?;
let headers = c.get_headers().clone(); // or use the reference directly
```

If you stored the value from `get_session_id` and send it yourself as a
cookie, add the `ESPSessionState=` prefix.

## 0.4.0

- Replaced the blocking API with async.
- Added the JSON API client (`JsonMC`).
- Removed the `lazy_static` dependency and updated dependencies.
- Added more examples.
