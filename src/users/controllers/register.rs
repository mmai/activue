use actix::Addr;
use actix_web::{web, Error, HttpResponse, ResponseError};
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Local};
use futures::Future;

use lettre_email::Email;
use lettre::{SmtpClient, Transport};
use lettre::file::FileTransport;
use lettre::smtp::authentication::{Credentials, Mechanism};
use lettre::sendmail::SendmailTransport;

use crate::wiring::DbExecutor;

use crate::users::repository::register_handler::{RegisterUser, ValidateUser};
use crate::users::models::{SlimUser, User};

// ---------------- Register Action------------

// UserData is used to extract data from a post request by the client
#[derive(Debug, Deserialize)]
pub struct UserData {
    pub email: String,
    pub login: String,
    pub password: String,
}

pub fn register(
    user_data: web::Json<UserData>,
    db: web::Data<Addr<DbExecutor>>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let user_data = user_data.into_inner();
    let hashed = hash(user_data.password, DEFAULT_COST).expect("Hashing error");
    let register_user = RegisterUser {
        email: user_data.email,
        login: user_data.login,
        password: hashed,
    };
    db.send(register_user)
        .from_err()
        .and_then(|db_response| match db_response {
            Ok(user) => {
                let res = send_confirmation(&user);
                Ok(HttpResponse::Ok().json(res))
            }
            Err(err) => Ok(err.error_response()),
        })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendmailResult {
    result: bool
}

fn send_confirmation(user: &SlimUser) -> SendmailResult {
    let key = dotenv::var("SECRET_KEY").unwrap();
    let sending_email = std::env::var("SENDING_EMAIL_ADDRESS")
        .expect("SENDING_EMAIL_ADDRESS must be set");
    let base_url = dotenv::var("BASE_URL").unwrap_or_else(|_| "localhost".to_string());
    let recipient = user.email.as_str();
    let link = format!("{}{}", user.login, key);
    let hashlink = hash(link , DEFAULT_COST).expect("Hashing error");
    let url = format!("{}/register/{}?l={}", base_url, hashlink, user.login);
    let email_body = format!(
        "Please click on the link below to complete registration. <br/>
         <a href=\"{url}\">{url}</a> <br>
         your Invitation expires on <strong>{date}</strong>",
         url = url,
         date = user.expires_at
            .format("%I:%M %p %A, %-d %B, %C%y")
            .to_string()
    );
    // println!("{}", email_body);

    let email = Email::builder()
        .from((sending_email, "Activue"))
        .to(recipient)
        .subject("You have been invited to join Activue")
        .html(email_body)
        .build();
    assert!(email.is_ok());

    // let smtp_login = dotenv::var("SMTP_LOGIN").unwrap_or_else(|_| "user".to_string());
    // let smtp_pass = dotenv::var("SMTP_PASSWORD").unwrap_or_else(|_| "password".to_string());
    // let smtp_server = dotenv::var("SMTP_SERVER").unwrap_or_else(|_| "smtp.localhost".to_string()); 
    // let creds = Credentials::new( smtp_login, smtp_pass );
    // let mut mailer = SmtpClient::new_simple(&smtp_server)
    //     .unwrap()
    //     .credentials(creds)
    //     .transport();

    // let mut mailer = SmtpClient::new_unencrypted_localhost().unwrap().transport();
    let sendmail = dotenv::var("SENDMAIL").unwrap_or_else(|_| "/usr/sbin/sendmail".to_string()); 
    let mut mailer = SendmailTransport::new_with_command(sendmail);

    let result = mailer.send(email.unwrap().into());

    match result {
        Ok(res) => SendmailResult {result: true} ,
        Err(error) => {
            println!("error \n {:#?}", error);
            SendmailResult {result: false}
        }
    }
}

struct ValidateResult {
    message: String,
    result: bool
}

// ---------------- Validate action ------------
pub fn validate( data: web::Path<(String, String)>, db: web::Data<Addr<DbExecutor>>,) 
    -> impl Future<Item = HttpResponse, Error = Error> {
    let key = dotenv::var("SECRET_KEY").unwrap();
    let hashlink = data.0;
    let login = data.1;
    let local_hashlink = format!("{}{}", login, key);
    if (hashlink != local_hashlink){
        return Ok(HttpResponse::Ok().json(ValidateResult { result: false, message: "Incorrect link".as_str() }));
    }
    db.send(ValidateUser { login: login })
        .from_err()
        .and_then(|db_response| match db_response {
            Ok(user) => {
                Ok(HttpResponse::Ok().json(ValidateResult { result: true, message: "User activated".as_str() }))
            }
            Err(err) => Ok(HttpResponse::Ok().json(ValidateResult { result: false, message: "User not found".as_str() }))
        })
}

