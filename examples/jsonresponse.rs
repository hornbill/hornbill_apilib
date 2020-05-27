use hornbill_apilib::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PingCheck {
    #[serde(rename = "@status")]
    pub status: bool,
    pub params: Params,
}

#[derive(Debug, Deserialize)]
pub struct Params {
    #[serde(rename = "stageName")]
    pub stage_name: String,
    #[serde(rename = "nextStage")]
    pub next_stage: i64,
    #[serde(rename = "serviceParamsChecksum")]
    pub service_params_checksum: Option<String>,
}

fn main() {
    //Get the url we will be connecting to for our instance. We should only ever need to call this once.
    let url = get_url_from_name("demo").expect("We did not get a url for our instance");
    //Create a xmlmc object we will use to send data to our instance. It requires the url we feteched earlier.
    let mut c = Xmlmc::new(&url).expect("Could not create client");

    //We tell the xmlmc object that we want to get a json response rather than the usual xml.
    c.set_json_response(true);

    //We will call the system::pingCheck API https://mdh-p01-api.hornbill.com/demo/xmlmc/system/?op=pingCheck

    // This requires one input paramets of stage which is an unsignedint
    c.set_param("stage", "1").expect("Could not set stage");

    //We now invoke the call and check the response if its Ok() we save the string result to res. If it was an Err() we print the error and return.
    let res = match c.invoke("system", "pingCheck") {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    //We now have a valid json string response in res which we can print
    println!("{}", &res);

    //We now need to Deserialize it into something we can use. We will use serde_json for this https://github.com/serde-rs/json

    let v: PingCheck = match serde_json::from_str(&res) {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    //We can debug print our struct
    println!("{:?}", v);

    //We can also access individual elements inside the struct
    println!("{}", v.params.next_stage);

    //We actully have an optional field service_params_checksum which you cannot call directly as you have to check if there is a value there.
    //This will not work and wont compile
    //println!("{}", v.params.service_params_checksum);

    if let Some(i) = v.params.service_params_checksum {
        println!("{}", i);
    } else {
        println!("We did not get a value for service_params_checksum");
    }
}
