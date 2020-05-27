use hornbill_apilib::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PingCheck {
    #[serde(rename = "status")]
    pub status: String,
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
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").expect("We did not get a url for our instance");

    //We then create our xmlmc object that we can use to query our instance.
    let mut c = Xmlmc::new(&url).expect("Could not create client");

    //We are going to pick a simple API that does not actully require a login https://api.hornbill.com/system/?op=pingCheck

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

    //We now have a valid xml string response in res which we can print
    println!("{}", &res);

    //We now need to Deserialize it into something we can use. We will use serde-xml-rs for this https://github.com/RReverser/serde-xml-rs

    let v: PingCheck = match serde_xml_rs::from_reader(res.as_bytes()) {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    //YOu can now print some of the values inside or PingCheck struct.
    println!("{}", v.status);
    println!("{}", v.params.stage_name);
    println!("{}", v.params.next_stage);
}
