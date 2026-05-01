use super::types::{AiError, AiProvider};

const SERVICE_PREFIX: &str = "progest-ai";
const USERNAME: &str = "api-key";

fn service_name(provider: AiProvider) -> String {
    format!("{SERVICE_PREFIX}-{}", provider.as_str())
}

fn entry(provider: AiProvider) -> Result<keyring::Entry, AiError> {
    keyring::Entry::new(&service_name(provider), USERNAME)
        .map_err(|e| AiError::KeychainError(e.to_string()))
}

/// Store an API key in the OS keychain.
pub fn store_api_key(provider: AiProvider, key: &str) -> Result<(), AiError> {
    entry(provider)?
        .set_password(key)
        .map_err(|e| AiError::KeychainError(e.to_string()))
}

/// Retrieve an API key from the OS keychain.
pub fn get_api_key(provider: AiProvider) -> Result<String, AiError> {
    entry(provider)?.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => AiError::NoApiKey {
            provider: provider.as_str().into(),
        },
        other => AiError::KeychainError(other.to_string()),
    })
}

/// Delete the stored API key for `provider`.
pub fn delete_api_key(provider: AiProvider) -> Result<(), AiError> {
    let ent = entry(provider)?;
    match ent.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AiError::KeychainError(e.to_string())),
    }
}

/// Check whether a key is stored without retrieving the secret.
pub fn has_api_key(provider: AiProvider) -> bool {
    entry(provider)
        .and_then(|e| {
            e.get_password()
                .map_err(|e| AiError::KeychainError(e.to_string()))
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_format() {
        assert_eq!(service_name(AiProvider::Anthropic), "progest-ai-anthropic");
        assert_eq!(service_name(AiProvider::OpenAi), "progest-ai-openai");
    }

    // Real keychain tests are OS-dependent and require user interaction
    // on some platforms. Run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore = "requires OS keychain access"]
    fn store_get_delete_round_trip() {
        let provider = AiProvider::Anthropic;
        let key = "sk-test-12345";

        store_api_key(provider, key).unwrap();
        assert!(has_api_key(provider));

        let got = get_api_key(provider).unwrap();
        assert_eq!(got, key);

        delete_api_key(provider).unwrap();
        assert!(!has_api_key(provider));

        let err = get_api_key(provider).unwrap_err();
        assert!(matches!(err, AiError::NoApiKey { .. }));
    }

    #[test]
    #[ignore = "requires OS keychain access"]
    fn delete_nonexistent_is_ok() {
        delete_api_key(AiProvider::OpenAi).unwrap();
    }
}
