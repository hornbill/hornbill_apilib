use hornbill_apilib::*;

fn main() {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").expect("We did not get a url for our instance");

    //We then create our xmlmc object that we can use to query our instance.
    let mut c = Xmlmc::new(&url).expect("Could not create client");

    //You could also hardcode the url of your instance rather than using get_url_from_name
    //let mut c = Xmlmc::new("http://hhq-p02-api.hornbill.com/hornbill/").expect("Could not create client");

    //We Need to login to the instance There are two ways of doing this

    //The first is to set an apikey like this
    //c.set_apikey(s);

    //The second is using the session::userLogon API https://mdh-p01-api.hornbill.com/demo/xmlmc/session/?op=userLogon
    //we are going to do this one

    //We need to send both userId and our password base64 encoded.
    c.set_param("UserId", "administrator").expect("ERROR");
    c.set_param("password", &base64::encode("password"))
        .expect("ERROR");

    //We Can print what we are going to send to the server before sending using get_params()
    println!("{}", c.get_params());

    //This invokes a http request using our parameters to session::logon and if we get a valid response stores it in the res string otherwise it returns.
    let res = match c.invoke("session", "userLogon") {
        Ok(s) => s,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    //You can check what http status code
    println!("http status code: {}", c.get_status_code());

    //When you logon to your instance
    println!("SessionId: {}", c.get_session_id());
    println!("{}", res);
}
