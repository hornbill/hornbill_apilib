use regex::Regex;
use serde::Deserialize;
use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// The xmlmc struct which contains all the methods required to interact with the hornbill api.
#[derive(Clone)]
pub struct Xmlmc {
    server: String,
    paramsxml: String,
    statuscode: u16,
    timeout: u64,
    count: u64,
    session_id: String,
    api_key: String,
    trace: String,
    jsonresp: bool,
    user_agent: String,
    copy_headers: bool,
    headers: http::header::HeaderMap,
    client: reqwest::Client,
}

impl fmt::Debug for Xmlmc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Xmlmc")
            .field("server", &self.server)
            .field("paramsxml", &self.paramsxml)
            .field("statuscode", &self.statuscode)
            .field("timeout", &self.timeout)
            .field("count", &self.count)
            .field("session_id", &"[redacted]")
            .field("api_key", &"[redacted]")
            .field("trace", &self.trace)
            .field("jsonresp", &self.jsonresp)
            .field("user_agent", &self.user_agent)
            .field("copy_headers", &self.copy_headers)
            .field("headers", &self.headers)
            .finish()
    }
}

/// The new Json way of using Hornbill API's. This sohuld be the prefered method as it is faster and better supported.
#[derive(Clone)]
pub struct JsonMC {
    server: String,
    timeout: u64,
    count: u64,
    session_id: String,
    api_key: String,
    trace: String,
    user_agent: String,
    client: reqwest::Client,
}

impl fmt::Debug for JsonMC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonMC")
            .field("server", &self.server)
            .field("timeout", &self.timeout)
            .field("count", &self.count)
            .field("session_id", &"[redacted]")
            .field("api_key", &"[redacted]")
            .field("trace", &self.trace)
            .field("user_agent", &self.user_agent)
            .finish()
    }
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
pub struct Attributes {
    key: String,
    value: String,
}

#[derive(Deserialize,Serialize, Debug,Clone)]
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
    /// ```ignore
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

/// Error details returned by the Hornbill JSON API when a call's `status` is `false`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResponseState {
    pub code: String,
    pub service: Option<String>,
    pub operation: Option<String>,
    pub error: String,
}

/// Errors that can occur when talking to a Hornbill instance.
#[derive(Debug)]
pub enum ApiError {
    /// The underlying HTTP request failed (network error, TLS error, etc).
    Request(reqwest::Error),
    /// The server responded with a non-200 HTTP status code.
    NonOkStatus(u16),
    /// The response body could not be serialized/deserialized as JSON.
    Serialization(serde_json::Error),
    /// A zoneinfo lookup (`get_url_from_name`) completed but did not report success.
    Zoneinfo(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Request(e) => write!(f, "http request failed: {}", e),
            ApiError::NonOkStatus(code) => write!(f, "non 200 status code: {}", code),
            ApiError::Serialization(e) => write!(f, "json (de)serialization failed: {}", e),
            ApiError::Zoneinfo(message) => write!(f, "zoneinfo lookup failed: {}", message),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Request(e) => Some(e),
            ApiError::Serialization(e) => Some(e),
            ApiError::NonOkStatus(_) | ApiError::Zoneinfo(_) => None,
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

/// Finds the `ESPSessionState=` cookie in a `Set-Cookie` header set, if present.
fn extract_session_id(headers: &http::HeaderMap) -> Option<String> {
    headers.get_all("Set-Cookie").iter().find_map(|value| {
        let s = value.to_str().ok()?;
        if s.contains("ESPSessionState=") {
            s.split(';').next().map(|token| token.to_owned())
        } else {
            None
        }
    })
}

impl JsonMC {
    pub fn new(s: &str ) -> Result<JsonMC, Box<dyn std::error::Error>> {
        let jsonclient = reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent("rust_apilib/1.1")
            .build()?;
        Ok(JsonMC {
            server: format!("{}/", s),
            timeout: 30,
            count: 0,
            session_id: "".to_owned(),
            api_key: "".to_owned(),
            trace: "".to_owned(),
            user_agent: "rust_apilib/1.1".to_owned(),
            client: jsonclient,
        })

    }

    /// You can use this to change the http client's request timeout. This defaults to 30 seconds.
    /// ```ignore
    /// c.set_timeout(60);
    /// ```
    pub fn set_timeout(&mut self, seconds: u64) {
        self.timeout = seconds;

        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout))
            .user_agent(&self.user_agent)
            .build()
        {
            self.client = client;
        }
    }


    pub fn parse_request_as<T: DeserializeOwned>(s: &str) -> Result<Request<T>, serde_json::Error> {
    serde_json::from_str(s)
    }

    pub fn parse_response_as<T: DeserializeOwned>(s: &str) -> Result<Response<T>, serde_json::Error> {
    serde_json::from_str(s)
    }

    /// You can use this to get the currently set sessionId. This sessionId will be generated when you call userLogon or guestLogon and stored in the Xmlmc object
    /// for all other calls after this.
    /// ```ignore
    /// let session_id = c.get_session_id();
    /// ```
    pub fn get_session_id(&self) -> String {
        self.session_id.to_owned()
    }

    /// Sends a request and deserializes the response params as `R`. The request params type `P`
    /// is inferred from `request`; `R` usually needs to be given explicitly, e.g.
    /// `c.invoke::<_, MyParams>(service, method, &request).await`.
    pub async fn invoke<P: Serialize, R: DeserializeOwned>(
        &mut self,
        service: &str,
        method: &str,
        request: &Request<P>,
    ) -> Result<Response<R>, ApiError> {
        //We should set the http header and response code

        // TODO: trace is not yet wired up for the JSON client (self.trace is unused here).

        let url = format!("{}/{}/?method={}", self.server, service, method);

        let mut req = self
            .client
            .post(&url)
            .body(serde_json::to_string(request)?)
            .header("Content-Type", "application/json")
            .header("User-Agent", &self.user_agent)
            .header("Cookie", &self.session_id)
            .header("Accept", "application/json");

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("ESP-APIKEY {}", self.api_key));
        }

        let result = req.send().await?;

        self.count += 1;

        if result.status() != http::StatusCode::OK {
            return Err(ApiError::NonOkStatus(result.status().as_u16()));
        }

        if let Some(session_id) = extract_session_id(result.headers()) {
            self.session_id = session_id;
        }

        let text = result.text().await?;

        Ok(serde_json::from_str(&text)?)
    }
}

impl Xmlmc {
    /// You can can create a xmlmc object that can be used to send data to your hornbill instance
    /// This will be created with a default timeout of 30 seconds and user_agent of "rust_apilib/1.1"
    /// ```ignore
    /// let mut c = Xmlmc::new(&url).expect("Could not create client");
    /// ```
    pub fn new(s: &str) -> Result<Xmlmc, Box<dyn std::error::Error>> {
        let xmlmcclient = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("rust_apilib/1.1")
            .build()?;

        Ok(Xmlmc {
            server: format!("{}/", s),
            paramsxml: "".to_owned(),
            statuscode: 0,
            timeout: 30,
            count: 0,
            session_id: "".to_owned(),
            api_key: "".to_owned(),
            trace: "".to_owned(),
            jsonresp: false,
            user_agent: "rust_apilib/1.1".to_owned(),
            copy_headers: false,
            headers: http::header::HeaderMap::new(),
            client: xmlmcclient,
        })
    }

    /// You can add parameters to the xml you will be sending to the server.
    /// Any not utf8 text in value will be replace with the utf8 replacement character.
    /// ```ignore
    /// c.set_param("username","admin");
    /// ```
    pub fn set_param(&mut self, key: &str, value: &str) -> Result<(), &str> {
        //empty names are not valid
        if key.is_empty() {
            return Err("Xml element cannot be empty");
        }
        //Make sure its valid xml
        if !check_valid_xml(key) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        let cleaned = xmlencode(value);

        //We neet to check that the input is valid utf8 otherwise we cannot add it to a rust string.
        //We are going to
        let clean_value = String::from_utf8_lossy(cleaned.as_bytes());

        self.paramsxml = format!("{}<{}>{}</{}>", self.paramsxml, key, clean_value, key);
        Ok(())
    }

    /// You can set multiple
    pub fn set_param_attr(
        &mut self,
        key: &str,
        value: &str,
        attribs: Vec<Attributes>,
    ) -> Result<(), &str> {
        //empty names are not valid
        if key.is_empty() {
            return Err("Xml element cannot be empty");
        }
        //Make sure its valid xml
        if !check_valid_xml(key) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        let cleaned = xmlencode(value);
        let clean_value = String::from_utf8_lossy(cleaned.as_bytes());

        let mut attrs = String::new();
        for i in attribs {
            if i.key.is_empty() {
                return Err("Xml attribute name cannot be empty");
            }
            if !check_valid_xml(&i.key) {
                return Err("Xml attribute name can only contain alphanumeric and underscores");
            }
            let cleaned_attr = xmlencode(&i.value);
            let clean_attr_value = String::from_utf8_lossy(cleaned_attr.as_bytes());

            attrs.push_str(&format!(" {}=\"{}\" ", i.key, clean_attr_value));
        }

        self.paramsxml = format!(
            "{}<{}{}>{}</{}>",
            self.paramsxml, key, attrs, clean_value, key
        );
        Ok(())
    }

    /// You can use this to open an xml element in your xml output to the server
    /// ```ignore
    /// c.open_element("userObject");
    /// ```
    /// This will append
    /// ```ignore
    /// <userObject>
    /// ```
    pub fn open_element(&mut self, element: &str) -> Result<(), &str> {
        if element.is_empty() {
            return Err("Xml element cannot be empty");
        }
        if !check_valid_xml(element) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        self.paramsxml = format!("{}<{}>", self.paramsxml, element);
        Ok(())
    }

    /// You can use this to close an xml element in your xml output to the server
    /// ```ignore
    /// c.close_element("userObject");
    /// ```
    /// This will append
    /// ```ignore
    /// </userObject>
    /// ```
    pub fn close_element(&mut self, element: &str) -> Result<(), &str> {
        if element.is_empty() {
            return Err("Xml element cannot be empty");
        }
        if !check_valid_xml(element) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        self.paramsxml = format!("{}</{}>", self.paramsxml, element);
        Ok(())
    }

    /// You can use this to return the full xml we would be sending to the server
    /// ```ignore
    /// let xml_output = c.get_params();
    /// ```
    pub fn get_params(&self) -> String {
        if self.paramsxml.is_empty() {
            "".to_string()
        } else {
            format!("<params>{}</params>", self.paramsxml)
        }
    }

    /// You can use this to clear the contents of the xml you would send to the server.
    /// This is automtically called at the end of invoke so you can reuse the connection and send more requests.
    /// ```ignore
    /// c.clear_params()
    /// ```
    pub fn clear_params(&mut self) {
        self.paramsxml = "".to_string();
    }

    /// You can use this to set the useragent string that is sent to the hornbill server. This defaults to "rust_apilib/1.1"
    /// You should set this to something unique for you so we can see who is calling our api endpoints.
    /// ```ignore
    /// c.set_user_agent("demo_ldapimport/1.1");
    /// ```
    pub fn set_user_agent(&mut self, user: &str) {
        self.user_agent = user.to_string();
        self.rebuild_client();
    }

    /// You can use this to change the http client's request timeout. This defaults to 30 seconds.
    /// ```ignore
    /// c.set_timeout(60);
    /// ```
    pub fn set_timeout(&mut self, seconds: u64) {
        self.timeout = seconds;
        self.rebuild_client();
    }

    fn rebuild_client(&mut self) {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout))
            .user_agent(&self.user_agent)
            .build()
        {
            self.client = client;
        }
    }

    /// You can use this is ask for a json response from the server.
    /// It sets the Accept header to "text/json" so it knows to response with json otherwise it uses xml.
    /// ```ignore
    /// c.set_json_response(true);
    /// ```
    pub fn set_json_response(&mut self, b: bool) {
        self.jsonresp = b;
    }

    /// You can use this to get the currently set sessionId. This sessionId will be generated when you call userLogon or guestLogon and stored in the Xmlmc object
    /// for all other calls after this.
    /// ```ignore
    /// let session_id = c.get_session_id();
    /// ```
    pub fn get_session_id(&self) -> String {
        self.session_id.to_owned()
    }

    /// You can use this to set an APIkey <https://wiki.hornbill.com/index.php/API_keys> that can be used to identify youeself rather than the logon APIS.
    /// ```ignore
    /// c.set_apikey("1234567890");
    /// ```
    pub fn set_apikey(&mut self, s: &str) {
        self.api_key = s.to_owned();
    }
    /// You can use this to set a session_id that you have retrieved after calling userLogon or guestLogon
    /// ```ignore
    /// c.set_sessionid("1234567890");
    /// ```
    pub fn set_sessionid(&mut self, s: &str) {
        self.session_id = s.to_owned();
    }

    /// You can use this to set a a trace identifier. This can then be used to identify in logging this exact api call.
    /// ```ignore
    /// c.set_trace("0987654321zxc");
    /// ```
    pub fn set_trace(&mut self, s: &str) {
        self.trace = s.to_owned();
    }
    /// You can use this to tell the library to copy out all headers recieved back from the server for later use.
    /// You can then use the get_headers() method to view the headers after the invoke call.
    /// ```ignore
    /// c.set_copy_headers(true);
    /// ```
    pub fn set_copy_headers(&mut self, s: bool) {
        self.copy_headers = s;
        //We blank the headers so we dont leak them to another request.
        self.headers = http::header::HeaderMap::new();
    }
    /// You can use this to check the last http status number the server returned from an invoke call.
    /// ```ignore
    /// let status = c.get_status_code();
    /// ```
    pub fn get_status_code(&self) -> u16 {
        self.statuscode
    }
    /// You can use this to get the currently set url for the server you will be connecting to.
    /// ```ignore
    /// let server_url = c.get_server_url();
    /// ```
    pub fn get_server_url(&self) -> String {
        self.server.clone()
    }
    /// You can use this to get the number of http requests that have been made by this xmlmc object.
    /// ```ignore
    /// let counter = c.get_count();
    /// ```
    pub fn get_count(&self) -> u64 {
        self.count
    }
    /// You can use this to get the headers that were sent by the server for the last http call. You will need to call set_copy_headers(true) before any invoke
    /// call so that we save the headers.
    /// check out the responseheaders example to see how to query the headers.
    /// ```ignore
    /// let headers = c.get_headers();
    /// ```
    pub fn get_headers(&self) -> http::header::HeaderMap {
        self.headers.clone()
    }

    /// You can use this to make the http call to the server with the xml you have built. The result will either contain a Ok(string) with the response body in
    /// or an Err(ApiError) describing what failed.
    /// ```ignore
    /// let headers = c.invoke("service", "method").await;
    /// ```
    pub async fn invoke(&mut self, service: &str, method: &str) -> Result<String, ApiError> {
        //We should set the http header and response code

        //Set a tracing varible
        let mut trace = String::new();
        if !self.trace.is_empty() {
            trace = format!("/{}", self.trace);
        }

        let mut body = format!(
            "<methodCall service=\"{}\" method=\"{}\" trace=\"goApi{}\">",
            service, method, trace
        );

        if self.paramsxml.is_empty() {
            body += "</methodCall>";
        } else {
            body = format!(
                "{}\n<params>{}\n</params></methodCall>",
                body, self.paramsxml
            );
        }

        let url = format!("{}/{}/?method={}", self.server, service, method);

        let mut req = self
            .client
            .post(&url)
            .body(body)
            .header("Content-Type", "text/xmlmc")
            .header("User-Agent", &self.user_agent)
            .header("Cookie", &self.session_id);

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("ESP-APIKEY {}", self.api_key));
        }

        if self.jsonresp {
            req = req.header("Accept", "text/json");
        }

        let result = req.send().await?;

        self.count += 1;
        self.statuscode = result.status().as_u16();

        if self.copy_headers {
            self.headers = result.headers().clone();
        }
        self.clear_params();

        if result.status() != http::StatusCode::OK {
            return Err(ApiError::NonOkStatus(result.status().as_u16()));
        }

        if let Some(session_id) = extract_session_id(result.headers()) {
            self.session_id = session_id;
        }

        Ok(result.text().await?)
    }
}

fn check_valid_xml(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new("^[a-zA-Z0-9_]*$").unwrap());
    re.is_match(text)
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
/// then can reuse the url for any Xmlmc objects you create.
/// ```ignore
/// let url = get_url_from_name("demo").await;
/// ```
pub async fn get_url_from_name(key: &str) -> Result<String, ApiError> {
    get_url_from_name_impl(key, "https://files.hornbill.com", "https://files.hornbill.co").await
}

// Split out from get_url_from_name so tests can point primary_base/backup_base at a mock
// server instead of the real fileserver hosts.
async fn get_url_from_name_impl(
    key: &str,
    primary_base: &str,
    backup_base: &str,
) -> Result<String, ApiError> {
    let mut url = format!("{}/instances/{}/zoneinfo", primary_base, key);
    let backup_url = format!("{}/instances/{}/zoneinfo", backup_base, key);

    // Check fileserver hosting zoneinfo and switch to backup if anything goes wrong
    let probe_client = reqwest::Client::new();
    if let Ok(response) = probe_client.get(&url).send().await {
        if response.status() != reqwest::StatusCode::OK {
            url = backup_url;
        }
    }

    let xmlmcclient = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("reqwest-http/1.1")
        .build()?;

    let body = xmlmcclient.get(&url).send().await?.text().await?;

    let deserialized: Root = serde_json::from_str(&body)?;

    //Check we got a successful repsonse from server.
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
            vec![Attributes {
                key: "attr1".to_string(),
                value: "attr'value1".to_string(),
            }],
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

        assert_eq!(
            extract_session_id(&headers),
            Some("ESPSessionState=xyz123".to_string())
        );
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

    use wiremock::matchers::{header, method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

        assert_eq!(client.get_session_id(), "ESPSessionState=abc123");
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
        client.set_timeout(0); // 0 seconds is effectively "immediately expired"

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

        let response: Response<PingParams> = client
            .invoke("system", "pingCheck", &request)
            .await
            .unwrap();

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

        let response: Response<serde_json::Value> = client
            .invoke("system", "pingCheck", &request)
            .await
            .unwrap();

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
            .invoke::<_, serde_json::Value>("system", "pingCheck", &request)
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

        let response: Response<serde_json::Value> = client
            .invoke("session", "userLogon", &request)
            .await
            .unwrap();

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
            .invoke::<_, serde_json::Value>("system", "pingCheck", &request)
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

        let _: Response<serde_json::Value> = client
            .invoke("system", "pingCheck", &request)
            .await
            .unwrap();

        assert_eq!(client.get_session_id(), "ESPSessionState=jsonabc");
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
