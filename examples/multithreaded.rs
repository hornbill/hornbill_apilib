use base64::{engine::general_purpose, Engine as _};
use hornbill_apilib::*;
use serde_json::json;

//Creates its own JsonMC client and logs it in against session::userLogon. Returning the client
//(rather than just the session id) shows that each task ends up with its own independently
//authenticated connection, not a session id shared between them.
async fn logon(url: String) -> Result<JsonMC, String> {
    let mut c = JsonMC::new(&url).map_err(|e| e.to_string())?;

    let request = Request::new(
        "session",
        "userLogon",
        json!({
            "userId": "administrator",
            "password": general_purpose::STANDARD.encode("password"),
        }),
    );

    let response: Response<serde_json::Value> = c
        .invoke("session", "userLogon", &request)
        .await
        .map_err(|e| e.to_string())?;

    if response.status {
        Ok(c)
    } else {
        let message = response
            .state
            .map(|s| s.error)
            .unwrap_or_else(|| "unknown error".to_string());
        Err(format!("logon failed: {message}"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //We get the url of our instance. We should only ever do this once.
    let url = get_url_from_name("demo").await?;

    //tokio::spawn hands each logon to the multi-threaded runtime's worker pool, so both http
    //requests are genuinely in flight on separate threads at the same time rather than one
    //after the other.
    let task1 = tokio::spawn(logon(url.clone()));
    let task2 = tokio::spawn(logon(url.clone()));

    //task.await only fails if the task itself panicked or was cancelled; the actual logon
    //Result is handled below so both outcomes get printed regardless of which one lands first.
    for (name, result) in [("Client 1", task1.await?), ("Client 2", task2.await?)] {
        match result {
            Ok(client) => println!("{name} logged in, session id: {}", client.get_session_id()),
            Err(e) => println!("{name} failed to log in: {e}"),
        }
    }

    Ok(())
}
