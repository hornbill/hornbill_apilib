use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::time::Duration;

const DEFAULT_USER_AGENT: &str = concat!("rust_apilib/", env!("CARGO_PKG_VERSION"));
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const SESSION_COOKIE_PREFIX: &str = "ESPSessionState=";
/// Prefix for the trace attribute sent with every xmlmc methodCall, so calls made through this
/// library can be picked out in the server logs.
const TRACE_PREFIX: &str = "rustApi";

/// The http side of a Hornbill client, shared by Xmlmc and JsonMC: connection settings, session
/// and api key handling, and the details of the last response.
#[derive(Clone)]
struct Transport {
    server: String,
    // `server` parsed once up front, so building each request url can't fail.
    base_url: reqwest::Url,
    statuscode: u16,
    timeout: u64,
    count: u64,
    session_id: String,
    api_key: String,
    user_agent: String,
    copy_headers: bool,
    headers: http::header::HeaderMap,
    client: reqwest::Client,
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("server", &self.server)
            .field("statuscode", &self.statuscode)
            .field("timeout", &self.timeout)
            .field("count", &self.count)
            .field("session_id", &"[redacted]")
            .field("api_key", &"[redacted]")
            .field("user_agent", &self.user_agent)
            .field("copy_headers", &self.copy_headers)
            .field("headers", &self.headers)
            .finish()
    }
}

impl Transport {
    fn new(server: &str) -> Result<Transport, ApiError> {
        let server = normalise_server_url(server);
        let base_url = reqwest::Url::parse(&server)
            .map_err(|e| ApiError::InvalidUrl(format!("{}: {}", server, e)))?;
        if base_url.cannot_be_a_base() {
            return Err(ApiError::InvalidUrl(format!(
                "{}: not a valid server url",
                server
            )));
        }

        Ok(Transport {
            server,
            base_url,
            statuscode: 0,
            timeout: DEFAULT_TIMEOUT_SECS,
            count: 0,
            session_id: String::new(),
            api_key: String::new(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            copy_headers: false,
            headers: http::header::HeaderMap::new(),
            client: build_client(DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT)?,
        })
    }

    // Both setters only update their setting once the new client has built, so a failure leaves
    // the client exactly as it was.
    fn set_timeout(&mut self, seconds: u64) -> Result<(), ApiError> {
        self.client = build_client(seconds, &self.user_agent)?;
        self.timeout = seconds;
        Ok(())
    }

    fn set_user_agent(&mut self, user_agent: &str) -> Result<(), ApiError> {
        self.client = build_client(self.timeout, user_agent)?;
        self.user_agent = user_agent.to_owned();
        Ok(())
    }

    /// Posts `body` to service/method and returns the response body. The status code and (if
    /// enabled) headers are recorded before the status is checked, so they are still available
    /// after a non 200 response.
    async fn send(
        &mut self,
        service: &str,
        method: &str,
        body: String,
        content_type: &'static str,
        accept: Option<&'static str>,
    ) -> Result<String, ApiError> {
        let mut req = self
            .client
            .post(self.request_url(service, method))
            .body(body)
            .header("Content-Type", content_type);

        if let Some(accept) = accept {
            req = req.header("Accept", accept);
        }

        if !self.session_id.is_empty() {
            req = req.header(
                "Cookie",
                format!("{}{}", SESSION_COOKIE_PREFIX, self.session_id),
            );
        }

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("ESP-APIKEY {}", self.api_key));
        }

        let result = req.send().await?;

        self.count += 1;
        self.statuscode = result.status().as_u16();

        if self.copy_headers {
            self.headers = result.headers().clone();
        }

        if result.status() != http::StatusCode::OK {
            return Err(ApiError::NonOkStatus(result.status().as_u16()));
        }

        if let Some(session_id) = extract_session_id(result.headers()) {
            self.session_id = session_id;
        }

        Ok(result.text().await?)
    }

    /// Builds `<server><service>/?method=<method>`, percent-encoding service and method so
    /// characters like '&', '#' or '/' can't change the meaning of the url.
    fn request_url(&self, service: &str, method: &str) -> reqwest::Url {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("checked in Transport::new that the url can be a base")
            .pop_if_empty()
            .push(service)
            .push("");
        url.query_pairs_mut().append_pair("method", method);
        url
    }
}

fn build_client(timeout: u64, user_agent: &str) -> Result<reqwest::Client, ApiError> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .user_agent(user_agent)
        .build()?)
}

/// Generates the connection settings and response accessors that Xmlmc and JsonMC share. They
/// all delegate to the client's `transport` field.
macro_rules! transport_methods {
    () => {
        /// You can use this to change the http client's request timeout. This defaults to 30 seconds.
        /// If the http client cannot be rebuilt an error is returned and the previous timeout is kept.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// c.set_timeout(60)?;
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn set_timeout(&mut self, seconds: u64) -> Result<(), ApiError> {
            self.transport.set_timeout(seconds)
        }

        /// You can use this to set the useragent string that is sent to the hornbill server. This defaults to `rust_apilib/<crate version>`
        /// You should set this to something unique for you so we can see who is calling our api endpoints.
        /// If the value is not a valid http header value an error is returned and the previous user agent is kept.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// c.set_user_agent("demo_ldapimport/1.1")?;
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn set_user_agent(&mut self, user: &str) -> Result<(), ApiError> {
            self.transport.set_user_agent(user)
        }

        /// You can use this to set an APIkey <https://wiki.hornbill.com/index.php/API_keys> that can be used to identify yourself rather than the logon APIs.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// c.set_apikey("1234567890");
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn set_apikey(&mut self, s: &str) {
            self.transport.api_key = s.to_owned();
        }

        /// You can use this to get the currently set sessionId. This sessionId will be generated when you call userLogon or guestLogon and stored in the client object
        /// for all other calls after this. This is the bare id, without the `ESPSessionState=` cookie name.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// let session_id = c.get_session_id();
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn get_session_id(&self) -> String {
            self.transport.session_id.to_owned()
        }

        /// You can use this to set a session_id that you have retrieved after calling userLogon or guestLogon.
        /// Either the bare id or the full `ESPSessionState=<id>` cookie form is accepted.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// c.set_sessionid("1234567890");
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn set_sessionid(&mut self, s: &str) {
            self.transport.session_id = normalise_session_id(s);
        }

        /// You can use this to tell the library to copy out all headers received back from the server for later use.
        /// You can then use the get_headers() method to view the headers after the invoke call.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// c.set_copy_headers(true);
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn set_copy_headers(&mut self, s: bool) {
            self.transport.copy_headers = s;
            //We blank the headers so we dont leak them to another request.
            self.transport.headers = http::header::HeaderMap::new();
        }

        /// You can use this to check the last http status number the server returned from an invoke call.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// let status = c.get_status_code();
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn get_status_code(&self) -> u16 {
            self.transport.statuscode
        }

        /// You can use this to get the currently set url for the server you will be connecting to.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// let server_url = c.get_server_url();
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn get_server_url(&self) -> String {
            self.transport.server.clone()
        }

        /// You can use this to get the number of http requests that have been made by this client object.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// let counter = c.get_count();
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn get_count(&self) -> u64 {
            self.transport.count
        }

        /// You can use this to get the headers that were sent by the server for the last http call. You will need to call set_copy_headers(true) before any invoke
        /// call so that we save the headers.
        /// check out the responseheaders example to see how to query the headers.
        /// ```no_run
        /// # let mut c = hornbill_apilib::JsonMC::new("https://example.invalid")?;
        /// let headers = c.get_headers();
        /// // Clone it if you need to keep the headers after the next invoke call.
        /// let saved = headers.clone();
        /// # Ok::<(), hornbill_apilib::ApiError>(())
        /// ```
        pub fn get_headers(&self) -> &http::header::HeaderMap {
            &self.transport.headers
        }
    };
}

/// The xmlmc struct which contains all the methods required to interact with the hornbill api.
#[derive(Clone, Debug)]
pub struct Xmlmc {
    paramsxml: String,
    trace: String,
    jsonresp: bool,
    transport: Transport,
}

/// The new Json way of using Hornbill API's. This should be the preferred method as it is faster and better supported.
#[derive(Clone, Debug)]
pub struct JsonMC {
    transport: Transport,
}

#[derive(Debug, Deserialize)]
struct Root {
    pub zoneinfo: Zoneinfo,
}

#[derive(Debug, Deserialize)]
struct Zoneinfo {
    #[serde(rename(deserialize = "clusterFqn"))]
    pub _cluster_fqn: String,
    #[serde(rename(deserialize = "releaseStream"))]
    pub _release_stream: String,
    pub endpoint: String,
    #[serde(rename(deserialize = "apiEndpoint"))]
    pub api_endpoint: Option<String>,
    pub message: String,
}

/// Attributes that can be appended to an xml element.
#[derive(Debug, Clone)]
pub struct Attributes {
    key: String,
    value: String,
}

impl Attributes {
    /// Builds an attribute for use with set_param_attr.
    /// ```no_run
    /// # use hornbill_apilib::*;
    /// # let mut c = Xmlmc::new("https://example.invalid")?;
    /// c.set_param_attr("element", "value", vec![Attributes::new("attr1", "value1")])?;
    /// # Ok::<(), ApiError>(())
    /// ```
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Attributes {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Request<T = serde_json::Value> {
    // The server rejects `@csrf_token: null` outright ("not allowed to be null"); it must be
    // omitted entirely when there's no token, hence skip_serializing_if here.
    #[serde(rename = "@csrf_token", skip_serializing_if = "Option::is_none")]
    pub csrf_token: Option<String>,
    #[serde(rename = "@service")]
    pub service: String,
    #[serde(rename = "@method")]
    pub method: String,
    pub params: T,
}

impl<T> Request<T> {
    /// Builds a `Request` with `csrf_token` defaulted to `None`. Set `csrf_token` on the
    /// returned value directly if you need one.
    /// ```no_run
    /// # use hornbill_apilib::Request;
    /// let request = Request::new("session", "userLogon", serde_json::json!({"userId": "admin"}));
    /// ```
    pub fn new(service: impl Into<String>, method: impl Into<String>, params: T) -> Self {
        Request {
            csrf_token: None,
            service: service.into(),
            method: method.into(),
            params,
        }
    }
}

/// A Hornbill JSON API response. `params` is only present when `status` is `true`; when the
/// call fails at the API level (as opposed to an HTTP-level failure) `status` is `false` and
/// `state` carries the error details instead.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "T: Deserialize<'de>"))]
pub struct Response<T = serde_json::Value> {
    #[serde(rename = "@status")]
    pub status: bool,
    #[serde(default)]
    pub params: Option<T>,
    #[serde(default)]
    pub state: Option<ResponseState>,
}

impl<T> Response<T> {
    /// Turns the response into a `Result`: `Ok(params)` when `status` is `true` (params is `None`
    /// if the API returned none), or `Err(state)` when it is `false`.
    /// ```no_run
    /// # use hornbill_apilib::*;
    /// # #[derive(serde::Deserialize)]
    /// # struct MyParams {}
    /// # async fn run() -> Result<(), ApiError> {
    /// # let mut c = JsonMC::new("https://example.invalid")?;
    /// # let request = Request::new("system", "pingCheck", serde_json::json!({}));
    /// let params = c.invoke::<_, MyParams>(&request).await?.into_result()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_result(self) -> Result<Option<T>, ResponseState> {
        if self.status {
            Ok(self.params)
        } else {
            Err(self.state.unwrap_or_else(|| ResponseState {
                error: "api call failed without returning any error details".to_owned(),
                ..ResponseState::default()
            }))
        }
    }
}

/// Error details returned by the Hornbill JSON API when a call's `status` is `false`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResponseState {
    pub code: String,
    pub service: Option<String>,
    pub operation: Option<String>,
    pub error: String,
}

impl fmt::Display for ResponseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.service, &self.operation) {
            (Some(service), Some(operation)) => write!(
                f,
                "{}::{} failed ({}): {}",
                service, operation, self.code, self.error
            ),
            _ => write!(f, "api call failed ({}): {}", self.code, self.error),
        }
    }
}

impl std::error::Error for ResponseState {}

/// Errors that can occur when talking to a Hornbill instance.
#[derive(Debug)]
pub enum ApiError {
    /// The underlying HTTP request failed (network error, TLS error, etc), or the http client
    /// could not be built.
    Request(reqwest::Error),
    /// The server responded with a non-200 HTTP status code.
    NonOkStatus(u16),
    /// The response body could not be serialized/deserialized as JSON.
    Serialization(serde_json::Error),
    /// A zoneinfo lookup (`get_url_from_name`) completed but did not report success.
    Zoneinfo(String),
    /// An xml element or attribute name passed to Xmlmc was empty or contained characters other
    /// than letters, digits and underscores.
    InvalidXml(&'static str),
    /// The JSON API returned `status: false`. Produced by `?` on `Response::into_result`.
    Api(ResponseState),
    /// The server url passed to `Xmlmc::new` or `JsonMC::new` could not be parsed.
    InvalidUrl(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Request(e) => write!(f, "http request failed: {}", e),
            ApiError::NonOkStatus(code) => write!(f, "non 200 status code: {}", code),
            ApiError::Serialization(e) => write!(f, "json (de)serialization failed: {}", e),
            ApiError::Zoneinfo(message) => write!(f, "zoneinfo lookup failed: {}", message),
            ApiError::InvalidXml(message) => write!(f, "invalid xml: {}", message),
            ApiError::Api(state) => write!(f, "{}", state),
            ApiError::InvalidUrl(message) => write!(f, "invalid server url: {}", message),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Request(e) => Some(e),
            ApiError::Serialization(e) => Some(e),
            ApiError::Api(state) => Some(state),
            ApiError::NonOkStatus(_)
            | ApiError::Zoneinfo(_)
            | ApiError::InvalidXml(_)
            | ApiError::InvalidUrl(_) => None,
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Request(e)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Serialization(e)
    }
}

impl From<ResponseState> for ApiError {
    fn from(state: ResponseState) -> Self {
        ApiError::Api(state)
    }
}

/// Finds the `ESPSessionState=` cookie in a `Set-Cookie` header set and returns just its value, if present.
fn extract_session_id(headers: &http::HeaderMap) -> Option<String> {
    headers.get_all("Set-Cookie").iter().find_map(|value| {
        let s = value.to_str().ok()?;
        s.split(';')
            .map(str::trim)
            .find_map(|part| part.strip_prefix(SESSION_COOKIE_PREFIX))
            .map(|id| id.to_owned())
    })
}

/// Accepts either a bare session id or the full `ESPSessionState=<id>` cookie form (which is what
/// get_session_id used to return) and returns the bare id.
fn normalise_session_id(s: &str) -> String {
    s.strip_prefix(SESSION_COOKIE_PREFIX)
        .unwrap_or(s)
        .to_owned()
}

/// Normalises a server url so it always ends in exactly one '/'.
fn normalise_server_url(s: &str) -> String {
    format!("{}/", s.trim_end_matches('/'))
}

impl JsonMC {
    /// You can create a JsonMC object that can be used to send json requests to your hornbill instance.
    /// This will be created with a default timeout of 30 seconds and user_agent of `rust_apilib/<crate version>`.
    /// An error is returned if the url cannot be parsed.
    /// ```no_run
    /// # use hornbill_apilib::*;
    /// # let url = "https://example.invalid";
    /// let mut c = JsonMC::new(&url)?;
    /// # Ok::<(), ApiError>(())
    /// ```
    pub fn new(s: &str) -> Result<JsonMC, ApiError> {
        Ok(JsonMC {
            transport: Transport::new(s)?,
        })
    }

    transport_methods!();

    pub fn parse_request_as<T: DeserializeOwned>(s: &str) -> Result<Request<T>, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn parse_response_as<T: DeserializeOwned>(
        s: &str,
    ) -> Result<Response<T>, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Sends a request to the service and method named in the request and deserializes the
    /// response params as `R`. The request params type `P` is inferred from `request`; `R`
    /// usually needs to be given explicitly, e.g. `c.invoke::<_, MyParams>(&request).await`.
    pub async fn invoke<P: Serialize, R: DeserializeOwned>(
        &mut self,
        request: &Request<P>,
    ) -> Result<Response<R>, ApiError> {
        // TODO: trace is not yet supported by the JSON client.
        let body = serde_json::to_string(request)?;

        let text = self
            .transport
            .send(
                &request.service,
                &request.method,
                body,
                "application/json",
                Some("application/json"),
            )
            .await?;

        Ok(serde_json::from_str(&text)?)
    }
}

impl Xmlmc {
    /// You can create a xmlmc object that can be used to send data to your hornbill instance
    /// This will be created with a default timeout of 30 seconds and user_agent of `rust_apilib/<crate version>`.
    /// An error is returned if the url cannot be parsed.
    /// ```no_run
    /// # use hornbill_apilib::*;
    /// # let url = "https://example.invalid";
    /// let mut c = Xmlmc::new(&url)?;
    /// # Ok::<(), ApiError>(())
    /// ```
    pub fn new(s: &str) -> Result<Xmlmc, ApiError> {
        Ok(Xmlmc {
            paramsxml: String::new(),
            trace: String::new(),
            jsonresp: false,
            transport: Transport::new(s)?,
        })
    }

    transport_methods!();

    /// You can add parameters to the xml you will be sending to the server.
    /// The value is xml encoded for you.
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.set_param("username","admin")?;
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    pub fn set_param(&mut self, key: &str, value: &str) -> Result<(), ApiError> {
        validate_element_name(key)?;
        self.paramsxml
            .push_str(&format!("<{}>{}</{}>", key, xmlencode(value), key));
        Ok(())
    }

    /// You can add a parameter with one or more attributes to the xml you will be sending to the server.
    /// The value and attribute values are xml encoded for you.
    /// ```no_run
    /// # use hornbill_apilib::*;
    /// # let mut c = Xmlmc::new("https://example.invalid")?;
    /// c.set_param_attr("element", "value", vec![Attributes::new("attr1", "value1")])?;
    /// # Ok::<(), ApiError>(())
    /// ```
    pub fn set_param_attr(
        &mut self,
        key: &str,
        value: &str,
        attribs: Vec<Attributes>,
    ) -> Result<(), ApiError> {
        validate_element_name(key)?;

        let mut attrs = String::new();
        for i in attribs {
            validate_name(
                &i.key,
                "Xml attribute name cannot be empty",
                "Xml attribute name can only contain alphanumeric and underscores",
            )?;
            attrs.push_str(&format!(" {}=\"{}\" ", i.key, xmlencode(&i.value)));
        }

        self.paramsxml
            .push_str(&format!("<{}{}>{}</{}>", key, attrs, xmlencode(value), key));
        Ok(())
    }

    /// You can use this to open an xml element in your xml output to the server
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.open_element("userObject")?;
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    /// This will append
    /// ```xml
    /// <userObject>
    /// ```
    pub fn open_element(&mut self, element: &str) -> Result<(), ApiError> {
        validate_element_name(element)?;
        self.paramsxml.push_str(&format!("<{}>", element));
        Ok(())
    }

    /// You can use this to close an xml element in your xml output to the server
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.close_element("userObject")?;
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    /// This will append
    /// ```xml
    /// </userObject>
    /// ```
    pub fn close_element(&mut self, element: &str) -> Result<(), ApiError> {
        validate_element_name(element)?;
        self.paramsxml.push_str(&format!("</{}>", element));
        Ok(())
    }

    /// You can use this to return the full xml we would be sending to the server
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// let xml_output = c.get_params();
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    pub fn get_params(&self) -> String {
        if self.paramsxml.is_empty() {
            "".to_string()
        } else {
            format!("<params>{}</params>", self.paramsxml)
        }
    }

    /// You can use this to clear the contents of the xml you would send to the server.
    /// invoke does this automatically, whether or not the call succeeds, so you can reuse the
    /// object and send more requests.
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.clear_params();
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    pub fn clear_params(&mut self) {
        self.paramsxml.clear();
    }

    /// You can use this to ask for a json response from the server.
    /// It sets the Accept header to "text/json" so it knows to respond with json otherwise it uses xml.
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.set_json_response(true);
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    pub fn set_json_response(&mut self, b: bool) {
        self.jsonresp = b;
    }

    /// You can use this to set a trace identifier. This can then be used to identify in logging this exact api call.
    /// It is sent as `rustApi/<trace>`.
    /// ```no_run
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// c.set_trace("0987654321zxc");
    /// # Ok::<(), hornbill_apilib::ApiError>(())
    /// ```
    pub fn set_trace(&mut self, s: &str) {
        self.trace = s.to_owned();
    }

    /// You can use this to make the http call to the server with the xml you have built. The result will either contain a Ok(string) with the response body in
    /// or an Err(ApiError) describing what failed. The params are cleared whether or not the call succeeds.
    /// ```no_run
    /// # async fn run() -> Result<(), hornbill_apilib::ApiError> {
    /// # let mut c = hornbill_apilib::Xmlmc::new("https://example.invalid")?;
    /// let body = c.invoke("service", "method").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn invoke(&mut self, service: &str, method: &str) -> Result<String, ApiError> {
        let trace = if self.trace.is_empty() {
            String::new()
        } else {
            format!("/{}", self.trace)
        };

        let mut body = format!(
            "<methodCall service=\"{}\" method=\"{}\" trace=\"{}{}\">",
            xmlencode(service),
            xmlencode(method),
            TRACE_PREFIX,
            xmlencode(&trace)
        );

        let paramsxml = std::mem::take(&mut self.paramsxml);
        if paramsxml.is_empty() {
            body += "</methodCall>";
        } else {
            body = format!("{}\n<params>{}\n</params></methodCall>", body, paramsxml);
        }

        let accept = if self.jsonresp {
            Some("text/json")
        } else {
            None
        };

        self.transport
            .send(service, method, body, "text/xmlmc", accept)
            .await
    }
}

fn validate_element_name(name: &str) -> Result<(), ApiError> {
    validate_name(
        name,
        "Xml element cannot be empty",
        "Xml element can only contain alphanumeric and underscores",
    )
}

fn validate_name(
    name: &str,
    empty_message: &'static str,
    invalid_message: &'static str,
) -> Result<(), ApiError> {
    if name.is_empty() {
        return Err(ApiError::InvalidXml(empty_message));
    }
    if !check_valid_xml(name) {
        return Err(ApiError::InvalidXml(invalid_message));
    }
    Ok(())
}

fn check_valid_xml(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn xmlencode(my_str: &str) -> String {
    let mut s = String::with_capacity(my_str.len());

    for c in my_str.chars() {
        match c {
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            '\'' => s.push_str("&apos;"),
            '&' => s.push_str("&amp;"),
            _ => s.push(c),
        }
    }
    s
}

/// You can use this to get the https endpoint for your instance. You should only ever have to call this once per program and
/// then can reuse the url for any Xmlmc or JsonMC objects you create.
/// ```no_run
/// # async fn run() -> Result<(), hornbill_apilib::ApiError> {
/// let url = hornbill_apilib::get_url_from_name("demo").await?;
/// # Ok(())
/// # }
/// ```
pub async fn get_url_from_name(key: &str) -> Result<String, ApiError> {
    get_url_from_name_impl(
        key,
        "https://files.hornbill.com",
        "https://files.hornbill.co",
    )
    .await
}

// Split out from get_url_from_name so tests can point primary_base/backup_base at a mock
// server instead of the real fileserver hosts.
async fn get_url_from_name_impl(
    key: &str,
    primary_base: &str,
    backup_base: &str,
) -> Result<String, ApiError> {
    let primary_url = format!("{}/instances/{}/zoneinfo", primary_base, key);
    let backup_url = format!("{}/instances/{}/zoneinfo", backup_base, key);

    let xmlmcclient = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("reqwest-http/1.1")
        .build()?;

    // Use the primary fileserver's response if it is healthy, otherwise switch to the backup if
    // anything goes wrong (non 200, connection failure, timeout).
    let response = match xmlmcclient.get(&primary_url).send().await {
        Ok(response) if response.status() == reqwest::StatusCode::OK => response,
        _ => xmlmcclient.get(&backup_url).send().await?,
    };

    let body = response.text().await?;

    let deserialized: Root = serde_json::from_str(&body)?;

    //Check we got a successful response from server.
    if deserialized.zoneinfo.message == "Success" {
        if let Some(endpoint) = deserialized.zoneinfo.api_endpoint {
            return Ok(endpoint);
        }
        //manually adding xmlmc/ in case zoneInfo is on the old version
        return Ok(deserialized.zoneinfo.endpoint + "xmlmc/");
    }
    Err(ApiError::Zoneinfo(deserialized.zoneinfo.message))
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_client() {
        let mut x = super::Xmlmc::new("http://hhq-p02-api.hornbill.com/demo/xmlmc").unwrap();

        assert_eq!(
            x.get_server_url(),
            "http://hhq-p02-api.hornbill.com/demo/xmlmc/"
        );
        let _ = x.open_element("first");
        assert_eq!(x.get_params(), "<params><first></params>");
        let _ = x.set_param("element1", "Value1");
        assert_eq!(
            x.get_params(),
            "<params><first><element1>Value1</element1></params>"
        );
        let _ = x.close_element("first");
        assert_eq!(
            x.get_params(),
            "<params><first><element1>Value1</element1></first></params>"
        );

        let _ = x.set_param("£$%£$£$£$_~()", "test");
        assert_eq!(
            x.get_params(),
            "<params><first><element1>Value1</element1></first></params>"
        );

        //This should be blank with no <params></params>
        x.clear_params();
        assert_eq!(x.get_params(), "");

        //Add attributes.
        let _ = x.set_param_attr(
            "test2",
            "value2",
            vec![Attributes::new("attr1", "attr'value1")],
        );
        assert_eq!(
            x.get_params(),
            "<params><test2 attr1=\"attr&apos;value1\" >value2</test2></params>"
        );
    }

    #[test]
    fn xmlencode_escapes_reserved_characters() {
        assert_eq!(
            xmlencode(r#"<tag attr="a's">&"#),
            "&lt;tag attr=&quot;a&apos;s&quot;&gt;&amp;"
        );
        assert_eq!(xmlencode("plain text"), "plain text");
    }

    #[test]
    fn check_valid_xml_accepts_alphanumeric_and_underscore() {
        assert!(check_valid_xml("userObject_1"));
        assert!(check_valid_xml(""));
    }

    #[test]
    fn check_valid_xml_rejects_special_characters() {
        assert!(!check_valid_xml("user-object"));
        assert!(!check_valid_xml("<tag>"));
        assert!(!check_valid_xml("£$%"));
    }

    #[test]
    fn extract_session_id_finds_esp_session_cookie_among_others() {
        let mut headers = http::HeaderMap::new();
        headers.append(
            "Set-Cookie",
            http::HeaderValue::from_static("OtherCookie=abc; Path=/"),
        );
        headers.append(
            "Set-Cookie",
            http::HeaderValue::from_static("ESPSessionState=xyz123; Path=/; HttpOnly"),
        );

        assert_eq!(extract_session_id(&headers), Some("xyz123".to_string()));
    }

    #[test]
    fn extract_session_id_returns_none_when_absent() {
        let mut headers = http::HeaderMap::new();
        headers.append(
            "Set-Cookie",
            http::HeaderValue::from_static("OtherCookie=abc; Path=/"),
        );

        assert_eq!(extract_session_id(&headers), None);
    }

    #[test]
    fn extract_session_id_returns_none_when_no_cookies() {
        assert_eq!(extract_session_id(&http::HeaderMap::new()), None);
    }

    #[test]
    fn request_serializes_with_at_prefixed_keys() {
        let req = Request {
            csrf_token: Some("token123".to_string()),
            service: "session".to_string(),
            method: "userLogon".to_string(),
            params: serde_json::json!({"userId": "admin"}),
        };

        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["@csrf_token"], "token123");
        assert_eq!(value["@service"], "session");
        assert_eq!(value["@method"], "userLogon");
        assert_eq!(value["params"]["userId"], "admin");
    }

    #[test]
    fn request_omits_csrf_token_key_when_none() {
        // The server rejects `@csrf_token: null` outright, so the key must be absent entirely
        // rather than present with a JSON null.
        let req = Request::new("session", "userLogon", serde_json::json!({}));

        let value = serde_json::to_value(&req).unwrap();
        assert!(!value.as_object().unwrap().contains_key("@csrf_token"));
    }

    #[test]
    fn response_deserializes_from_hornbill_shaped_json() {
        let json = r#"{"@status": true, "params": {"stageName": "one", "nextStage": 2}}"#;
        let response: Response<serde_json::Value> = serde_json::from_str(json).unwrap();

        assert!(response.status);
        let params = response.params.unwrap();
        assert_eq!(params["stageName"], "one");
        assert_eq!(params["nextStage"], 2);
        assert!(response.state.is_none());
    }

    #[test]
    fn response_deserializes_failure_shape_without_params() {
        let json = r#"{"@status": false, "state": {"code": "0202", "service": "session", "operation": "userLogon", "error": "bad credentials"}}"#;
        let response: Response<serde_json::Value> = serde_json::from_str(json).unwrap();

        assert!(!response.status);
        assert!(response.params.is_none());
        let state = response.state.unwrap();
        assert_eq!(state.code, "0202");
        assert_eq!(state.error, "bad credentials");
    }

    #[test]
    fn api_error_display_messages() {
        assert_eq!(
            ApiError::NonOkStatus(404).to_string(),
            "non 200 status code: 404"
        );
        assert_eq!(
            ApiError::Zoneinfo("Instance not found".to_string()).to_string(),
            "zoneinfo lookup failed: Instance not found"
        );
    }

    #[test]
    fn api_error_serialization_variant_has_source() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = ApiError::from(json_err);

        assert!(matches!(err, ApiError::Serialization(_)));
        assert!(std::error::Error::source(&err).is_some());
    }

    use wiremock::matchers::{body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Returns a url nothing is listening on. Dropping a MockServer doesn't give us one, because
    /// wiremock pools servers and hands the "dropped" one straight to another test.
    fn unreachable_uri() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{}", port)
    }

    /// Mounts a userLogon that hands out session "s1", and a pingCheck that only succeeds when
    /// that session is sent back.
    async fn mount_session_round_trip(mock_server: &MockServer, body: &str) {
        Mock::given(method("POST"))
            .and(query_param("method", "userLogon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "ESPSessionState=s1; Path=/; HttpOnly")
                    .set_body_string(body),
            )
            .mount(mock_server)
            .await;
        Mock::given(method("POST"))
            .and(query_param("method", "pingCheck"))
            .and(header("Cookie", "ESPSessionState=s1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(mock_server)
            .await;
    }

    #[tokio::test]
    async fn xmlmc_sends_captured_session_on_next_call() {
        let mock_server = MockServer::start().await;
        mount_session_round_trip(&mock_server, "ok").await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("session", "userLogon").await.unwrap();
        // Unmatched requests get a 404, so this only succeeds if the cookie was sent.
        client.invoke("system", "pingCheck").await.unwrap();
    }

    #[tokio::test]
    async fn jsonmc_sends_captured_session_on_next_call() {
        let mock_server = MockServer::start().await;
        mount_session_round_trip(&mock_server, r#"{"@status":true}"#).await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let logon = Request::new("session", "userLogon", serde_json::json!({}));
        let _: Response = client.invoke(&logon).await.unwrap();
        let ping = Request::new("system", "pingCheck", serde_json::json!({}));
        let _: Response = client.invoke(&ping).await.unwrap();
    }

    #[tokio::test]
    async fn session_is_kept_when_later_responses_have_no_session_cookie() {
        let mock_server = MockServer::start().await;
        mount_session_round_trip(&mock_server, "ok").await;
        // A failing call that tries to hand out a different session must not replace ours.
        Mock::given(method("POST"))
            .and(query_param("method", "fails"))
            .respond_with(
                ResponseTemplate::new(500)
                    .append_header("Set-Cookie", "ESPSessionState=other; Path=/"),
            )
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("session", "userLogon").await.unwrap();

        client.invoke("system", "pingCheck").await.unwrap();
        assert_eq!(client.get_session_id(), "s1");

        assert!(client.invoke("system", "fails").await.is_err());
        assert_eq!(client.get_session_id(), "s1");

        // And it is still sent afterwards.
        client.invoke("system", "pingCheck").await.unwrap();
    }

    #[test]
    fn debug_output_redacts_credentials() {
        let mut xml = Xmlmc::new("http://example.invalid").unwrap();
        xml.set_apikey("secret-api-key");
        xml.set_sessionid("secret-session");
        let mut json = JsonMC::new("http://example.invalid").unwrap();
        json.set_apikey("secret-api-key");
        json.set_sessionid("secret-session");

        for output in [format!("{:?}", xml), format!("{:?}", json)] {
            assert!(!output.contains("secret-api-key"), "{}", output);
            assert!(!output.contains("secret-session"), "{}", output);
            assert!(output.contains("[redacted]"), "{}", output);
        }
    }

    #[tokio::test]
    async fn xmlmc_sends_expected_request_body_and_headers() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_param("stage", "1").unwrap();
        client.invoke("system", "pingCheck").await.unwrap();
        client.invoke("system", "pingCheck").await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(
            String::from_utf8(requests[0].body.clone()).unwrap(),
            "<methodCall service=\"system\" method=\"pingCheck\" trace=\"rustApi\">\n<params><stage>1</stage>\n</params></methodCall>"
        );
        // Params were cleared by the first invoke, so there is no <params> element at all.
        assert_eq!(
            String::from_utf8(requests[1].body.clone()).unwrap(),
            "<methodCall service=\"system\" method=\"pingCheck\" trace=\"rustApi\"></methodCall>"
        );
        assert_eq!(
            requests[0].headers.get("Content-Type").unwrap(),
            "text/xmlmc"
        );
        assert_eq!(requests[0].url.path(), "/system/");
        assert_eq!(requests[0].url.query(), Some("method=pingCheck"));
    }

    #[tokio::test]
    async fn jsonmc_sends_expected_request_body_and_headers() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({"stage": 1}));
        let _: Response = client.invoke(&request).await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        // Exact match, so this also checks that @csrf_token is absent on the wire.
        assert_eq!(
            body,
            serde_json::json!({"@service": "system", "@method": "pingCheck", "params": {"stage": 1}})
        );
        assert_eq!(
            requests[0].headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(
            requests[0].headers.get("Accept").unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn xmlmc_set_json_response_controls_accept_header() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("system", "pingCheck").await.unwrap();
        client.set_json_response(true);
        client.invoke("system", "pingCheck").await.unwrap();
        client.set_json_response(false);
        client.invoke("system", "pingCheck").await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let accept = |i: usize| {
            requests[i]
                .headers
                .get("Accept")
                .map(|v| v.to_str().unwrap().to_owned())
        };
        // reqwest adds its own default Accept, so check it is not text/json rather than absent.
        assert_ne!(accept(0).as_deref(), Some("text/json"));
        assert_eq!(accept(1).as_deref(), Some("text/json"));
        assert_ne!(accept(2).as_deref(), Some("text/json"));
    }

    #[tokio::test]
    async fn service_and_method_are_percent_encoded_in_url() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("my service/x", "a&b=c#d").await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests[0].url.path(), "/my%20service%2Fx/");
        let query: Vec<(String, String)> = requests[0].url.query_pairs().into_owned().collect();
        assert_eq!(query, vec![("method".to_string(), "a&b=c#d".to_string())]);
    }

    #[test]
    fn new_rejects_invalid_server_urls() {
        assert!(matches!(
            Xmlmc::new("not a url"),
            Err(ApiError::InvalidUrl(_))
        ));
        assert!(matches!(
            JsonMC::new("mailto:someone"),
            Err(ApiError::InvalidUrl(_))
        ));
    }

    #[tokio::test]
    async fn default_user_agent_includes_crate_version() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header(
                "User-Agent",
                concat!("rust_apilib/", env!("CARGO_PKG_VERSION")),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("system", "pingCheck").await.unwrap();
    }

    #[tokio::test]
    async fn copied_headers_are_replaced_each_call_and_cleared_when_disabled() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("method", "first"))
            .respond_with(ResponseTemplate::new(200).append_header("X-First", "1"))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(query_param("method", "second"))
            .respond_with(ResponseTemplate::new(200).append_header("X-Second", "2"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_copy_headers(true);
        client.invoke("system", "first").await.unwrap();
        assert!(client.get_headers().contains_key("X-First"));

        client.invoke("system", "second").await.unwrap();
        assert!(client.get_headers().contains_key("X-Second"));
        assert!(!client.get_headers().contains_key("X-First"));

        client.set_copy_headers(false);
        assert!(client.get_headers().is_empty());
        client.invoke("system", "first").await.unwrap();
        assert!(client.get_headers().is_empty());
    }

    #[tokio::test]
    async fn counters_unchanged_when_connection_fails() {
        let mut client = Xmlmc::new(&unreachable_uri()).unwrap();
        let err = client.invoke("system", "pingCheck").await.unwrap_err();

        assert!(matches!(err, ApiError::Request(_)));
        assert_eq!(client.get_count(), 0);
        assert_eq!(client.get_status_code(), 0);
    }

    #[test]
    fn xml_builder_handles_nesting_multiple_attributes_and_unicode() {
        let mut x = Xmlmc::new("http://example.invalid").unwrap();
        x.open_element("user").unwrap();
        x.set_param_attr(
            "name",
            "Zoë 日本",
            vec![Attributes::new("a", "1"), Attributes::new("b", "x&y")],
        )
        .unwrap();
        x.close_element("user").unwrap();

        assert_eq!(
            x.get_params(),
            "<params><user><name a=\"1\"  b=\"x&amp;y\" >Zoë 日本</name></user></params>"
        );
        assert!(!check_valid_xml("café"));
    }

    #[test]
    fn parse_helpers_accept_valid_json_and_ignore_unknown_fields() {
        #[derive(serde::Deserialize)]
        struct Params {
            a: i64,
        }

        let request = JsonMC::parse_request_as::<serde_json::Value>(
            r#"{"@service": "system", "@method": "pingCheck", "params": {"stage": 1}}"#,
        )
        .unwrap();
        assert_eq!(request.service, "system");
        assert_eq!(request.method, "pingCheck");
        assert!(request.csrf_token.is_none());

        // Extra fields from a newer server must not break parsing.
        let response = JsonMC::parse_response_as::<Params>(
            r#"{"@status": true, "params": {"a": 1, "extra": 2}, "flowcodeDebugState": {}}"#,
        )
        .unwrap();
        assert_eq!(response.params.unwrap().a, 1);

        assert!(JsonMC::parse_request_as::<serde_json::Value>(r#"{"@service": "s"#).is_err());
    }

    #[tokio::test]
    async fn jsonmc_sends_quotes_and_backslashes_intact() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let text = r#"He said "hi" \ then left"#;
        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({ "text": text }));
        let _: Response = client.invoke(&request).await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let raw = String::from_utf8(requests[0].body.clone()).unwrap();
        assert!(raw.contains(r#"He said \"hi\" \\ then left"#), "{}", raw);
        let body: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(body["params"]["text"], text);
    }

    #[tokio::test]
    async fn xmlmc_invoke_returns_body_on_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("method", "pingCheck"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<methodCallResult status=\"success\"></methodCallResult>"),
            )
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        let body = client.invoke("system", "pingCheck").await.unwrap();

        assert!(body.contains("success"));
        assert_eq!(client.get_status_code(), 200);
    }

    #[tokio::test]
    async fn xmlmc_invoke_returns_error_on_non_200() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        let err = client.invoke("system", "pingCheck").await.unwrap_err();

        assert!(matches!(err, ApiError::NonOkStatus(500)));
    }

    #[tokio::test]
    async fn xmlmc_invoke_captures_session_cookie() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "ESPSessionState=abc123; Path=/; HttpOnly")
                    .set_body_string("<methodCallResult status=\"success\"></methodCallResult>"),
            )
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.invoke("system", "pingCheck").await.unwrap();

        assert_eq!(client.get_session_id(), "abc123");
    }

    #[tokio::test]
    async fn xmlmc_invoke_sends_api_key_header() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Authorization", "ESP-APIKEY secret123"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_apikey("secret123");
        let body = client.invoke("system", "pingCheck").await.unwrap();

        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn jsonmc_invoke_sends_api_key_header() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Authorization", "ESP-APIKEY secret123"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        client.set_apikey("secret123");
        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let response: Response = client.invoke(&request).await.unwrap();

        assert!(response.status);
    }

    #[tokio::test]
    async fn jsonmc_set_user_agent_is_sent() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("User-Agent", "load-tester/1.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        client.set_user_agent("load-tester/1.0").unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let response: Response = client.invoke(&request).await.unwrap();

        assert!(response.status);
    }

    #[tokio::test]
    async fn xmlmc_set_timeout_actually_times_out_requests() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("ok")
                    .set_delay(std::time::Duration::from_millis(300)),
            )
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_timeout(0).unwrap(); // 0 seconds is effectively "immediately expired"

        let err = client.invoke("system", "pingCheck").await.unwrap_err();

        assert!(matches!(err, ApiError::Request(_)));
    }

    #[tokio::test]
    async fn jsonmc_invoke_parses_typed_response() {
        #[derive(serde::Deserialize)]
        struct PingParams {
            #[serde(rename = "stageName")]
            stage_name: String,
            #[serde(rename = "nextStage")]
            next_stage: i64,
        }

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"@status": true, "params": {"stageName": "one", "nextStage": 2}}"#,
            ))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({"stage": 1}));

        let response: Response<PingParams> = client.invoke(&request).await.unwrap();

        assert!(response.status);
        let params = response.params.unwrap();
        assert_eq!(params.stage_name, "one");
        assert_eq!(params.next_stage, 2);
    }

    #[tokio::test]
    async fn jsonmc_invoke_sends_application_json_content_type() {
        // The real Hornbill JSON API rejects `text/json` outright with a
        // "content-type not supported" error, so this must be `application/json`.
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"@status": true, "params": {}}"#),
            )
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let response: Response<serde_json::Value> = client.invoke(&request).await.unwrap();

        assert!(response.status);
    }

    #[tokio::test]
    async fn jsonmc_invoke_returns_error_on_non_200() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let err = client
            .invoke::<_, serde_json::Value>(&request)
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::NonOkStatus(403)));
    }

    #[tokio::test]
    async fn jsonmc_invoke_parses_api_level_failure_without_params() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"@status": false, "state": {"code": "0202", "service": "session", "operation": "userLogon", "error": "bad credentials"}}"#,
            ))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("session", "userLogon", serde_json::json!({}));

        let response: Response<serde_json::Value> = client.invoke(&request).await.unwrap();

        assert!(!response.status);
        assert!(response.params.is_none());
        assert_eq!(response.state.unwrap().error, "bad credentials");
    }

    #[tokio::test]
    async fn jsonmc_invoke_returns_error_on_malformed_json() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let err = client
            .invoke::<_, serde_json::Value>(&request)
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::Serialization(_)));
    }

    #[tokio::test]
    async fn jsonmc_invoke_captures_session_cookie() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "ESPSessionState=jsonabc; Path=/; HttpOnly")
                    .set_body_string(r#"{"@status": true, "params": {}}"#),
            )
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let _: Response<serde_json::Value> = client.invoke(&request).await.unwrap();

        assert_eq!(client.get_session_id(), "jsonabc");
    }

    #[tokio::test]
    async fn jsonmc_set_sessionid_is_sent_as_cookie() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Cookie", "ESPSessionState=preset"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        client.set_sessionid("preset");
        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let response: Response = client.invoke(&request).await.unwrap();

        assert!(response.status);
        assert_eq!(client.get_session_id(), "preset");
    }

    #[test]
    fn set_sessionid_accepts_legacy_cookie_form() {
        // get_session_id used to return the whole cookie, so callers may still pass that back in.
        let mut json = JsonMC::new("http://example.invalid").unwrap();
        json.set_sessionid("ESPSessionState=legacy");
        assert_eq!(json.get_session_id(), "legacy");

        let mut xml = Xmlmc::new("http://example.invalid").unwrap();
        xml.set_sessionid("ESPSessionState=legacy");
        assert_eq!(xml.get_session_id(), "legacy");
    }

    #[tokio::test]
    async fn no_cookie_header_sent_without_session() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut json = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let _: Response = json.invoke(&request).await.unwrap();

        let mut xml = Xmlmc::new(&mock_server.uri()).unwrap();
        xml.invoke("system", "pingCheck").await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for r in requests {
            assert!(r.headers.get("Cookie").is_none());
        }
    }

    #[tokio::test]
    async fn invoke_url_has_single_slashes_with_trailing_slash_server() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xmlmc/system/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        // get_url_from_name returns urls ending in "xmlmc/", so make sure that doesn't double up.
        let server = format!("{}/xmlmc/", mock_server.uri());

        let mut json = JsonMC::new(&server).unwrap();
        assert_eq!(
            json.get_server_url(),
            format!("{}/xmlmc/", mock_server.uri())
        );
        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let _: Response = json.invoke(&request).await.unwrap();

        let mut xml = Xmlmc::new(&server).unwrap();
        xml.invoke("system", "pingCheck").await.unwrap();
    }

    #[tokio::test]
    async fn xmlmc_escapes_method_call_attributes() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains(
                r#"trace="rustApi/a&quot; injected=&quot;x""#,
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_trace(r#"a" injected="x"#);
        let body = client.invoke("system", "pingCheck").await.unwrap();

        assert_eq!(body, "ok");
    }

    #[test]
    fn response_into_result_returns_params_on_success() {
        let response: Response =
            serde_json::from_str(r#"{"@status": true, "params": {"a": 1}}"#).unwrap();
        assert_eq!(response.into_result().unwrap().unwrap()["a"], 1);

        let response: Response = serde_json::from_str(r#"{"@status": true}"#).unwrap();
        assert!(response.into_result().unwrap().is_none());
    }

    #[test]
    fn response_into_result_returns_state_on_failure() {
        let response: Response = serde_json::from_str(
            r#"{"@status": false, "state": {"code": "0202", "service": "session", "operation": "userLogon", "error": "bad credentials"}}"#,
        )
        .unwrap();
        let state = response.into_result().unwrap_err();
        assert_eq!(state.error, "bad credentials");
        assert_eq!(
            state.to_string(),
            "session::userLogon failed (0202): bad credentials"
        );

        // `?` turns it into ApiError::Api.
        let err = ApiError::from(state);
        assert!(matches!(err, ApiError::Api(_)));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn response_into_result_without_state_still_errors() {
        let response: Response = serde_json::from_str(r#"{"@status": false}"#).unwrap();
        assert!(!response.into_result().unwrap_err().error.is_empty());
    }

    #[test]
    fn set_param_rejects_invalid_names_with_api_error() {
        let mut x = Xmlmc::new("http://example.invalid").unwrap();
        assert!(matches!(x.set_param("", "v"), Err(ApiError::InvalidXml(_))));
        assert!(matches!(
            x.open_element("a-b"),
            Err(ApiError::InvalidXml(_))
        ));
        assert!(matches!(
            x.set_param_attr("ok", "v", vec![Attributes::new("bad name", "v")]),
            Err(ApiError::InvalidXml(_))
        ));
        assert_eq!(x.get_params(), "");
    }

    #[tokio::test]
    async fn set_user_agent_rejects_invalid_value_and_keeps_previous() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("User-Agent", "good/1.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        client.set_user_agent("good/1.0").unwrap();
        assert!(client.set_user_agent("bad\nagent").is_err());

        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let response: Response = client.invoke(&request).await.unwrap();
        assert!(response.status);

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests[0].headers.get_all("User-Agent").iter().count(), 1);
    }

    #[tokio::test]
    async fn jsonmc_invoke_uses_service_and_method_from_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/session/"))
            .and(query_param("method", "userLogon"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("session", "userLogon", serde_json::json!({}));
        let response: Response = client.invoke(&request).await.unwrap();

        assert!(response.status);
    }

    #[tokio::test]
    async fn xmlmc_invoke_clears_params_even_on_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let mut client = Xmlmc::new(&mock_server.uri()).unwrap();
        client.set_param("stage", "1").unwrap();
        assert!(client.invoke("system", "pingCheck").await.is_err());

        assert_eq!(client.get_params(), "");
    }

    #[tokio::test]
    async fn jsonmc_tracks_status_code_and_count() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"@status":true}"#))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        assert_eq!(client.get_status_code(), 0);
        assert_eq!(client.get_count(), 0);
        assert_eq!(client.get_server_url(), format!("{}/", mock_server.uri()));

        let request = Request::new("system", "pingCheck", serde_json::json!({}));
        let _: Response = client.invoke(&request).await.unwrap();
        let _: Response = client.invoke(&request).await.unwrap();

        assert_eq!(client.get_status_code(), 200);
        assert_eq!(client.get_count(), 2);
    }

    #[tokio::test]
    async fn jsonmc_copies_headers_only_when_enabled() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("X-Test", "hello")
                    .set_body_string(r#"{"@status":true}"#),
            )
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let _: Response = client.invoke(&request).await.unwrap();
        assert!(client.get_headers().is_empty());

        client.set_copy_headers(true);
        let _: Response = client.invoke(&request).await.unwrap();
        assert_eq!(client.get_headers().get("X-Test").unwrap(), "hello");
    }

    #[tokio::test]
    async fn jsonmc_records_status_and_headers_on_non_200() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).append_header("Retry-After", "5"))
            .mount(&mock_server)
            .await;

        let mut client = JsonMC::new(&mock_server.uri()).unwrap();
        client.set_copy_headers(true);
        let request = Request::new("system", "pingCheck", serde_json::json!({}));

        let err = client
            .invoke::<_, serde_json::Value>(&request)
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::NonOkStatus(503)));
        assert_eq!(client.get_status_code(), 503);
        assert_eq!(client.get_headers().get("Retry-After").unwrap(), "5");
    }

    #[tokio::test]
    async fn get_url_from_name_uses_primary_when_healthy() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://primary.example/", "apiEndpoint": null, "message": "Success"}}"#,
            ))
            .mount(&primary)
            .await;

        let url = get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap();

        assert_eq!(url, "https://primary.example/xmlmc/");
    }

    #[tokio::test]
    async fn get_url_from_name_falls_back_when_primary_unhealthy() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&primary)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://backup.example/", "apiEndpoint": null, "message": "Success"}}"#,
            ))
            .mount(&backup)
            .await;

        let url = get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap();

        assert_eq!(url, "https://backup.example/xmlmc/");
    }

    #[tokio::test]
    async fn get_url_from_name_falls_back_when_primary_unreachable() {
        let primary_uri = unreachable_uri();
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://backup.example/", "apiEndpoint": null, "message": "Success"}}"#,
            ))
            .mount(&backup)
            .await;

        let url = get_url_from_name_impl("demo", &primary_uri, &backup.uri())
            .await
            .unwrap();

        assert_eq!(url, "https://backup.example/xmlmc/");
    }

    #[tokio::test]
    async fn get_url_from_name_errors_when_both_fileservers_unreachable() {
        let primary_uri = unreachable_uri();
        let backup_uri = unreachable_uri();

        let err = get_url_from_name_impl("demo", &primary_uri, &backup_uri)
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::Request(_)));
    }

    #[tokio::test]
    async fn get_url_from_name_errors_when_backup_also_fails() {
        let primary_uri = unreachable_uri();
        let backup = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&backup)
            .await;

        let err = get_url_from_name_impl("demo", &primary_uri, &backup.uri())
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::Serialization(_)));
    }

    #[tokio::test]
    async fn get_url_from_name_makes_one_request_when_primary_healthy() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://primary.example/", "apiEndpoint": null, "message": "Success"}}"#,
            ))
            .expect(1)
            .mount(&primary)
            .await;

        get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap();

        assert!(backup.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_url_from_name_prefers_api_endpoint_when_present() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://primary.example/", "apiEndpoint": "https://api.primary.example/xmlmc/", "message": "Success"}}"#,
            ))
            .mount(&primary)
            .await;

        let url = get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap();

        assert_eq!(url, "https://api.primary.example/xmlmc/");
    }

    #[tokio::test]
    async fn get_url_from_name_errors_when_zoneinfo_reports_failure() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"zoneinfo": {"clusterFqn": "c", "releaseStream": "s", "endpoint": "https://primary.example/", "apiEndpoint": null, "message": "Instance not found"}}"#,
            ))
            .mount(&primary)
            .await;

        let err = get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::Zoneinfo(ref msg) if msg == "Instance not found"));
    }

    #[tokio::test]
    async fn get_url_from_name_errors_on_malformed_response() {
        let primary = MockServer::start().await;
        let backup = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&primary)
            .await;

        let err = get_url_from_name_impl("demo", &primary.uri(), &backup.uri())
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::Serialization(_)));
    }
}
