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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //Get the url we will be connecting to for our instance. We should only ever need to call this once.
    let url = get_url_from_name("demo").await?;
    //Create a xmlmc object we will use to send data to our instance. It requires the url we fetched earlier.
    let mut c = Xmlmc::new(&url)?;

    //We tell the xmlmc object that we want to get a json response rather than the usual xml.
    c.set_json_response(true);

    //We will call the system::pingCheck API https://mdh-p01-api.hornbill.com/demo/xmlmc/system/?op=pingCheck

    // This requires one input parameter of stage which is an unsigned int
    c.set_param("stage", "1")?;

    //We now invoke the call and save the string result to res, otherwise the error propagates out of main.
    let res = c.invoke("system", "pingCheck").await?;

    //We now have a valid json string response in res which we can print
    println!("{}", res);

    //We now need to Deserialize it into something we can use. We will use serde_json for this https://github.com/serde-rs/json

    let v: PingCheck = serde_json::from_str(&res)?;

    //We can debug print our struct
    println!("{:?}", v);

    //We can also access individual elements inside the struct
    println!("{}", v.params.next_stage);

    //We actually have an optional field service_params_checksum which you cannot call directly as you have to check if there is a value there.
    //This will not work and wont compile
    //println!("{}", v.params.service_params_checksum);

    if let Some(i) = v.params.service_params_checksum {
        println!("{}", i);
    } else {
        println!("We did not get a value for service_params_checksum");
    }

    Ok(())
}
