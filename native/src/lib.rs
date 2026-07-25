mod application;
mod infrastructure;
mod service;

use application::file_upload::UploadResult as ApplicationUploadResult;
use bytes::Bytes;
use napi::bindgen_prelude::{Buffer, External};
use napi::{Error, Result, Status};
use napi_derive::napi;
use service::{Service, ServiceError};

#[napi(object)]
pub struct UploadResult {
    pub url: String,
    pub pathname: String,
    #[napi(js_name = "contentType")]
    pub content_type: String,
}

impl From<ApplicationUploadResult> for UploadResult {
    fn from(result: ApplicationUploadResult) -> Self {
        Self {
            url: result.url,
            pathname: result.pathname,
            content_type: result.content_type,
        }
    }
}

fn to_napi_error(error: ServiceError) -> Error {
    let status = match error {
        ServiceError::Configuration(_) => Status::InvalidArg,
        ServiceError::Upload(_) => Status::GenericFailure,
    };

    Error::new(status, error.to_string())
}

#[napi(js_name = "createService")]
pub async fn create_service() -> Result<External<Service>> {
    Service::new()
        .await
        .map(External::new)
        .map_err(to_napi_error)
}

#[napi(js_name = "uploadObject")]
pub async fn upload_object(
    service: &External<Service>,
    data: Buffer,
    filename: String,
    content_type: String,
) -> Result<UploadResult> {
    service
        .upload_object(Bytes::from(data.to_vec()), filename, content_type)
        .await
        .map(UploadResult::from)
        .map_err(to_napi_error)
}
