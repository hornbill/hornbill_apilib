use hornbill_apilib::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PingCheck {
    // serde-xml-rs 0.8+ requires attribute fields to be prefixed with `@`; `status` is an XML
    // attribute here (`<methodCallResult status="ok">`), not a child element.
    #[serde(rename = "@status")]
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").await?;

    //We then create our xmlmc object that we can use to query our instance.
    let mut c = Xmlmc::new(&url)?;

    //We are going to pick a simple API that does not actully require a login https://api.hornbill.com/system/?op=pingCheck

    // This requires one input paramets of stage which is an unsignedint
    c.set_param("stage", "1")?;

    //We now invoke the call and save the string result to res, otherwise the error propagates out of main.
    let res = c.invoke("system", "pingCheck").await?;

    //We now have a valid xml string response in res which we can print
    println!("{}", res);

    //We now need to Deserialize it into something we can use. We will use serde-xml-rs for this https://github.com/RReverser/serde-xml-rs

    let v: PingCheck = serde_xml_rs::from_reader(res.as_bytes())?;

    //YOu can now print some of the values inside or PingCheck struct.
    println!("{}", v.status);
    println!("{}", v.params.stage_name);
    println!("{}", v.params.next_stage);

    Ok(())
}
