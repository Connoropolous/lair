//! Ed25519 Signature Utilities
//! NOTE - temporarily using RING crate until we switch to sodoken

use crate::*;
use derive_more::*;

/// The 64 byte signature ed25519 public key.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, From, Into,
)]
#[allow(clippy::rc_buffer)]
pub struct SignEd25519PrivKey(pub Arc<Vec<u8>>);

impl From<Vec<u8>> for SignEd25519PrivKey {
    fn from(d: Vec<u8>) -> Self {
        Self(Arc::new(d))
    }
}

/// The 32 byte signature ed25519 public key.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, From, Into,
)]
#[allow(clippy::rc_buffer)]
pub struct SignEd25519PubKey(pub Arc<Vec<u8>>);

impl From<Vec<u8>> for SignEd25519PubKey {
    fn from(d: Vec<u8>) -> Self {
        Self(Arc::new(d))
    }
}

impl SignEd25519PubKey {
    /// Verify signature on given message with given public key.
    #[allow(clippy::rc_buffer)]
    pub async fn verify(
        &self,
        message: Arc<Vec<u8>>,
        signature: SignEd25519Signature,
    ) -> LairResult<bool> {
        internal::sign_ed25519::sign_ed25519_verify(
            self.clone(),
            message,
            signature,
        )
        .await
    }
}

/// The 64 byte detached ed25519 signature data.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, From, Into,
)]
#[allow(clippy::rc_buffer)]
pub struct SignEd25519Signature(pub Arc<Vec<u8>>);

impl From<Vec<u8>> for SignEd25519Signature {
    fn from(d: Vec<u8>) -> Self {
        Self(Arc::new(d))
    }
}

/// Generate a new random ed25519 signature keypair.
pub async fn sign_ed25519_keypair_new_from_entropy(
) -> LairResult<entry::EntrySignEd25519> {
    rayon_exec(move || {
        let sys_rand = ring::rand::SystemRandom::new();
        let mut priv_key = vec![0; 32];
        ring::rand::SecureRandom::fill(&sys_rand, &mut priv_key)
            .map_err(|e| format!("{:?}", e))?;
        let keypair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&priv_key)
                .map_err(|e| format!("{:?}", e))?;
        let pub_key = ring::signature::KeyPair::public_key(&keypair)
            .as_ref()
            .to_vec();
        Ok(entry::EntrySignEd25519 {
            priv_key: priv_key.into(),
            pub_key: pub_key.into(),
        })
    })
    .await
}

/// Generate detached signature bytes for given ed25519 priv key / message.
#[allow(clippy::rc_buffer)]
pub async fn sign_ed25519(
    priv_key: SignEd25519PrivKey,
    message: Arc<Vec<u8>>,
) -> LairResult<SignEd25519Signature> {
    rayon_exec(move || {
        let keypair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&priv_key)
                .map_err(|e| format!("{:?}", e))?;
        let signature = keypair.sign(&message);
        Ok(signature.as_ref().to_vec().into())
    })
    .await
}

/// Verify signature on given message with given public key.
#[allow(clippy::rc_buffer)]
pub async fn sign_ed25519_verify(
    pub_key: SignEd25519PubKey,
    message: Arc<Vec<u8>>,
    signature: SignEd25519Signature,
) -> LairResult<bool> {
    rayon_exec(move || {
        let pub_key = ring::signature::UnparsedPublicKey::new(
            &ring::signature::ED25519,
            &**pub_key,
        );
        Ok(pub_key.verify(&message, &signature).is_ok())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn it_can_sign_and_verify() {
        let msg = Arc::new(vec![0, 1, 2, 3]);

        let entry::EntrySignEd25519 { priv_key, pub_key } =
            sign_ed25519_keypair_new_from_entropy().await.unwrap();

        let sig = sign_ed25519(priv_key.clone(), msg.clone()).await.unwrap();

        assert!(
            sign_ed25519_verify(pub_key.clone(), msg.clone(), sig.clone(),)
                .await
                .unwrap()
        );

        let mut bad_sig = (**sig).clone();
        use std::num::Wrapping;
        bad_sig[0] = (Wrapping(bad_sig[0]) + Wrapping(1)).0;
        assert!(!sign_ed25519_verify(
            pub_key.clone(),
            msg.clone(),
            bad_sig.into(),
        )
        .await
        .unwrap());
    }
}
