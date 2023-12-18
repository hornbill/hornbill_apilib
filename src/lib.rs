#[macro_use]
extern crate lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::time::Duration;

/// The xmlmc struct which contains all the methods required to interact with the hornbill api.
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
    client: reqwest::blocking::Client,
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

impl Xmlmc {
    /// You can can create a xmlmc object that can be used to send data to your hornbill instance
    /// This will be created with a default timeout of 30 seconds and user_agent of "rust_apilib/1.1"
    /// ```ignore
    /// let mut c = Xmlmc::new(&url).expect("Could not create client");
    /// ```
    pub fn new(s: &str) -> Result<Xmlmc, Box<dyn std::error::Error>> {
        let xmlmcclient = reqwest::blocking::Client::builder()
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
        if !check_valid_xml(&key) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        let cleaned = xmlencode(value);

        //We neet to check that the input is valid utf8 otherwise we cannot add it to a rust string.
        //We are going to
        let clean_value = String::from_utf8_lossy(cleaned.as_bytes());

        self.paramsxml = format!("{}<{}>{}</{}>", &self.paramsxml, &key, &clean_value, &key);
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
        if !check_valid_xml(&key) {
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

            attrs.push_str(&format!(" {}=\"{}\" ", &i.key, &clean_attr_value));
        }

        self.paramsxml = format!(
            "{}<{}{}>{}</{}>",
            &self.paramsxml, &key, &attrs, &clean_value, &key
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
        if !check_valid_xml(&element) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        self.paramsxml = format!("{}<{}>", &self.paramsxml, element);
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
        if !check_valid_xml(&element) {
            return Err("Xml element can only contain alphanumeric and underscores");
        }
        self.paramsxml = format!("{}</{}>", &self.paramsxml, element);
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
            return format!("<params>{}</params>", self.paramsxml);
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

        let xmlmcclient = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.timeout))
            .user_agent(&self.user_agent)
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        self.client = xmlmcclient;
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
    /// or an Err(String) with the error message of what failed.
    /// ```ignore
    /// let headers = c.invoke();
    /// ```
    pub fn invoke(&mut self, service: &str, method: &str) -> Result<String, String> {
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
            body = body + "</methodCall>";
        } else {
            body = format!(
                "{}\n<params>{}\n</params></methodCall>",
                body, &self.paramsxml
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
            req = req.header("Authorization", format!("ESP-APIKEY {}", &self.api_key));
        }

        if self.jsonresp {
            req = req.header("Accept", "text/json");
        }

        let result = match req.send() {
            Ok(response) => response,
            Err(e) => return Err(e.to_string()),
        };

        self.count += 1;
        self.statuscode = result.status().as_u16();

        if self.copy_headers {
            self.headers = result.headers().clone();
        }
        self.clear_params();

        if result.status() != http::StatusCode::OK {
            //we return none for now but should probably set errors and status code.
            return Err("Non 200 Status code".to_string());
        }

        if let Some(cookie) = result.headers().get("Set-Cookie") {
            //split on ";" then pick the first element and set as sessionId, requires utf8 otherwise will not set value.
            if let Ok(c_string) = cookie.to_str() {
                for token in c_string.split(";") {
                    self.session_id = token.to_owned();
                    break;
                }
            }
        }

        match result.text() {
            Ok(s) => return Ok(s),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn check_valid_xml(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new("^[a-zA-Z0-9_]*$").unwrap();
    }
    RE.is_match(text)
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
/// let url = get_url_from_name("demo");
/// ```
pub fn get_url_from_name(key: &str) -> Option<String> {
    let mut url = format!("https://files.hornbill.com/instances/{}/zoneinfo", key);
    let _backup_url = format!("https://files.hornbill.co/instances/{}/zoneinfo", key);

    /// Checks fileserver hosting zoneinfo config file and switches to backup if anything goes wrong
    /// ```ignore
    /// zoneinfo_status_check("https://files.hornbill.com/instances/demo/zoneinfo", "https://files.hornbill.co/instances/demo/zoneinfo")
    /// ```
    fn zoneinfo_status_check(url: &mut String, backup_url: String) {
        let client = reqwest::blocking::Client::new();
        match client.get(&*url).send() {
            Ok(response) => {
                if response.status() != reqwest::StatusCode::OK {
                    *url = backup_url;
                }
            },
            Err(e) => {
                println!("{}", e);
            }
        };
    }
    zoneinfo_status_check(&mut url, _backup_url);

    let xmlmcclient = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("reqwest-http/1.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            println!("{}", e);
            return None;
        }
    };

    let response = match xmlmcclient.get(&url).send() {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return None;
        }
    };

    let body = match response.text() {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return None;
        } //TODO we need to tell client this did not work.
    };

    let deserialized: Root = match serde_json::from_str(&body) {
        Ok(s) => s,
        Err(_) => return None,
    };

    //Check we got a successful repsonse from server.
    if deserialized.zoneinfo.message == "Success" {
        if deserialized.zoneinfo.api_endpoint.is_some(){
            return deserialized.zoneinfo.api_endpoint;
        } 
        return Some(deserialized.zoneinfo.endpoint + "xmlmc/"); //manually adding xmlmc/ in case zoneInfo is on the old version
    }
    None
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
        let _ = x.clear_params();
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
}
