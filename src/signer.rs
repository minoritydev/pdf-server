use async_trait::async_trait;
use log::{debug, info};
use reqsign_core::{Context, OsEnv, ProvideCredential, ProvideCredentialChain, Result};
use reqsign_file_read_tokio::TokioFileRead;
use reqsign_http_send_reqwest::ReqwestHttpSend;
use reqsign_oracle::{
    Credential, DefaultCredentialProvider, EnvCredentialProvider, StaticCredentialProvider,
};

#[derive(Debug)]
struct LoggingProvider<P> {
    name: String,
    inner: P,
}

impl<P> LoggingProvider<P> {
    fn new(name: impl Into<String>, provider: P) -> Self {
        Self {
            name: name.into(),
            inner: provider,
        }
    }
}

#[async_trait]
impl<P> ProvideCredential for LoggingProvider<P>
where
    P: ProvideCredential<Credential = Credential> + Send + Sync,
{
    type Credential = Credential;

    async fn provide_credential(&self, ctx: &Context) -> Result<Option<Self::Credential>> {
        info!("Attempting to load credentials from: {}", self.name);
        match self.inner.provide_credential(ctx).await {
            Ok(Some(cred)) => {
                info!("Loaded credentials from: {}", self.name);
                debug!("User: {}", cred.user);
                Ok(Some(cred))
            }
            Ok(None) => {
                info!("No credentials found in: {}", self.name);
                Ok(None)
            }
            Err(e) => {
                info!("Error loading credentials from {}: {:?}", self.name, e);
                Err(e)
            }
        }
    }
}

pub struct SignerService {
    ctx: Context,
    chain: ProvideCredentialChain<Credential>,
}

impl SignerService {
    pub fn new() -> Self {
        let ctx = Context::new()
            .with_http_send(ReqwestHttpSend::default())
            .with_env(OsEnv);

        let chain = ProvideCredentialChain::new()
            .push(LoggingProvider::new("Environment", EnvCredentialProvider::new()));

        Self { ctx, chain }
    }

    pub async fn get_credential(&self) -> Result<Credential> {
        match self.chain.provide_credential(&self.ctx).await? {
            Some(cred) => Ok(cred),
            None => Err("No credentials found".into()),
        }
    }
}
