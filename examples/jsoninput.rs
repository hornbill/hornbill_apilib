use base64::{engine::general_purpose, Engine as _};
use hornbill_apilib::*;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").await?;

    //We then create our JsonMC object that we can use to query our instance using the JSON api rather than xml.
    let mut c = JsonMC::new(&url)?;

    //You could also hardcode the url of your instance rather than using get_url_from_name
    //let mut c = JsonMC::new("http://hhq-p02-api.hornbill.com/hornbill/")?;

    //We are using the session::userLogon API https://mdh-p01-api.hornbill.com/demo/xmlmc/session/?op=userLogon
    //but this time we build the request as json rather than xml.

    //We need to send both userId and our password base64 encoded.
    //STANDARD is used as no padding results in an error
    let request = Request::new(
        "session",
        "userLogon",
        json!({
            "userId": "administrator",
            "password": general_purpose::STANDARD.encode("password"),
        }),
    );

    //This invokes a http request using our request to session::userLogon (the service and method
    //are taken from the request). We don't have a strongly typed struct for the response params
    //here so we just deserialize them as a generic serde_json::Value.
    let response: Response<serde_json::Value> = c.invoke(&request).await?;

    //`params` is only present when status is true; on an API-level failure `state` carries the
    //error details instead (there is no `params` key in that case). into_result turns this into
    //a normal Result so you could also just use `?` here.
    match response.into_result() {
        Ok(params) => {
            //When you logon to your instance the session id is stored for later calls
            println!("SessionId: {}", c.get_session_id());
            println!("params: {}", params.unwrap_or_default());
        }
        Err(state) => println!("{}", state),
    }

    Ok(())
}
