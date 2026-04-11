/// OAuth2 credentials for the "Audit My Visitors" Google Cloud app.
///
/// These are set at compile time via environment variables:
///   GOOGLE_CLIENT_ID=xxx GOOGLE_CLIENT_SECRET=xxx cargo build --release
///
/// For desktop/installed apps Google explicitly documents that the client_secret
/// is NOT truly secret — it is a public identifier embedded in the binary.
/// See: https://developers.google.com/identity/protocols/oauth2/native-app
///
/// Until the Google Cloud app is registered, build with placeholder values and
/// the binary will print a clear error on first use.
pub const CLIENT_ID: &str = match option_env!("GOOGLE_CLIENT_ID") {
    Some(v) => v,
    None => "GOOGLE_CLIENT_ID_NOT_SET",
};

pub const CLIENT_SECRET: &str = match option_env!("GOOGLE_CLIENT_SECRET") {
    Some(v) => v,
    None => "GOOGLE_CLIENT_SECRET_NOT_SET",
};
