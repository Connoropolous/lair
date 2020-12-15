use futures::{future::FutureExt, stream::StreamExt};
use ghost_actor::dependencies::tracing;
use lair_keystore_api::actor::LairClientApiSender;
use lair_keystore_api::internal::crypto_box;

fn init_tracing() {
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .compact()
            .finish(),
    );
}

#[tokio::test(threaded_scheduler)]
async fn lair_integration_test() -> lair_keystore_api::LairResult<()> {
    init_tracing();

    let tmpdir = tempfile::tempdir().unwrap();
    std::env::set_var("LAIR_DIR", tmpdir.path());

    lair_keystore::execute_lair().await?;

    let config = lair_keystore_api::Config::builder()
        .set_root_path(tmpdir.path())
        .build();

    if let Err(e) = std::fs::metadata(config.get_socket_path()) {
        panic!(
            "could not read socket file!!: {:?} {:?}",
            config.get_socket_path(),
            e
        );
    }

    let spawn = || async {
        let (api_send, mut evt_recv) =
            lair_keystore_api::ipc::spawn_client_ipc(config.clone()).await?;

        tokio::task::spawn(async move {
            while let Some(msg) = evt_recv.next().await {
                match msg {
                    lair_keystore_api::actor::LairClientEvent::RequestUnlockPassphrase {
                        respond,
                        ..
                    } => {
                        respond.respond(Ok(
                            async move { Ok("passphrase".to_string()) }
                                .boxed()
                                .into(),
                        ));
                    }
                }
            }
        });

        lair_keystore_api::LairResult::<_>::Ok(api_send)
    };

    let api_send = spawn().await?;

    let api_send2 = spawn().await?;

    let info = api_send.lair_get_server_info().await?;
    assert_eq!("lair-keystore", &info.name);
    assert_eq!(lair_keystore::LAIR_VER, &info.version);

    let info = api_send2.lair_get_server_info().await?;
    assert_eq!("lair-keystore", &info.name);
    assert_eq!(lair_keystore::LAIR_VER, &info.version);

    assert_eq!(0, api_send.lair_get_last_entry_index().await?.0);
    assert_eq!(
        lair_keystore_api::actor::LairEntryType::Invalid,
        api_send.lair_get_entry_type(0.into()).await?,
    );

    let (cert_index, cert_sni, cert_digest) = api_send
        .tls_cert_new_self_signed_from_entropy(
            lair_keystore_api::actor::TlsCertOptions::default(),
        )
        .await?;

    assert_eq!(1, cert_index.0);
    assert_eq!(1, api_send.lair_get_last_entry_index().await?.0);
    assert_eq!(
        lair_keystore_api::actor::LairEntryType::TlsCert,
        api_send.lair_get_entry_type(1.into()).await?,
    );

    let (cert_sni2, cert_digest2) = api_send.tls_cert_get(cert_index).await?;
    assert_eq!(cert_sni, cert_sni2);
    assert_eq!(cert_digest, cert_digest2);

    let cert1 = api_send.tls_cert_get_cert_by_index(cert_index).await?;
    let cert2 = api_send.tls_cert_get_cert_by_sni(cert_sni).await?;
    let cert3 = api_send.tls_cert_get_cert_by_digest(cert_digest).await?;

    assert_eq!(cert1, cert2);
    assert_eq!(cert2, cert3);

    let pk1 = api_send.tls_cert_get_priv_key_by_index(cert_index).await?;
    let pk2 = api_send.tls_cert_get_priv_key_by_sni(cert_sni2).await?;
    let pk3 = api_send
        .tls_cert_get_priv_key_by_digest(cert_digest2)
        .await?;

    assert_eq!(pk1, pk2);
    assert_eq!(pk2, pk3);

    let (sign_index, sign_pub_key) =
        api_send.sign_ed25519_new_from_entropy().await?;

    assert_eq!(2, sign_index.0);
    assert_eq!(2, api_send.lair_get_last_entry_index().await?.0);
    assert_eq!(
        lair_keystore_api::actor::LairEntryType::SignEd25519,
        api_send.lair_get_entry_type(2.into()).await?,
    );

    let sign_pub_key2 = api_send.sign_ed25519_get(sign_index).await?;

    assert_eq!(sign_pub_key, sign_pub_key2);

    let data = std::sync::Arc::new(b"test-data".to_vec());

    let sign1 = api_send
        .sign_ed25519_sign_by_index(sign_index, data.clone())
        .await?;
    let sign2 = api_send
        .sign_ed25519_sign_by_pub_key(sign_pub_key.clone(), data.clone())
        .await?;

    assert_eq!(sign1, sign2);

    let sign3 = api_send2
        .sign_ed25519_sign_by_index(sign_index, data.clone())
        .await?;
    let sign4 = api_send2
        .sign_ed25519_sign_by_pub_key(sign_pub_key, data.clone())
        .await?;

    assert_eq!(sign2, sign3);
    assert_eq!(sign3, sign4);

    let (x25519_alice_index, x25519_alice_pub_key) = api_send.x25519_new_from_entropy().await?;

    assert_eq!(3, x25519_alice_index.0);
    assert_eq!(3, api_send.lair_get_last_entry_index().await?.0);
    assert_eq!(
        lair_keystore_api::actor::LairEntryType::X25519,
        api_send.lair_get_entry_type(3.into()).await?,
    );

    let x25519_alice_pub_key2 = api_send.x25519_get(x25519_alice_index).await?;

    assert_eq!(x25519_alice_pub_key, x25519_alice_pub_key2);

    let (x25519_bob_index, x25519_bob_pub_key) = api_send.x25519_new_from_entropy().await?;
    assert_eq!(4, x25519_bob_index.0);

    let data = std::sync::Arc::new(b"test-data".to_vec().into());

    // Encrypt a few times in a few ways.
    let crypto_box1 = api_send.crypto_box_by_index(x25519_alice_index, x25519_bob_pub_key.clone(), std::sync::Arc::new(crypto_box::CryptoBoxData { data: std::sync::Arc::clone(&data) })).await?;
    let crypto_box2 = api_send.crypto_box_by_pub_key(x25519_alice_pub_key.clone(), x25519_bob_pub_key.clone(), std::sync::Arc::new(crypto_box::CryptoBoxData { data: std::sync::Arc::clone(&data) })).await?;
    let crypto_box3 = api_send2.crypto_box_by_index(x25519_alice_index, x25519_bob_pub_key.clone(), std::sync::Arc::new(crypto_box::CryptoBoxData { data: std::sync::Arc::clone(&data) })).await?;
    let crypto_box4 = api_send2.crypto_box_by_pub_key(x25519_alice_pub_key.clone(), x25519_bob_pub_key.clone(), std::sync::Arc::new(crypto_box::CryptoBoxData { data: std::sync::Arc::clone(&data) })).await?;

    assert_ne!(crypto_box1.nonce, crypto_box2.nonce);
    assert_ne!(crypto_box1.nonce, crypto_box3.nonce);
    assert_ne!(crypto_box2.nonce, crypto_box4.nonce);
    assert_ne!(crypto_box1.encrypted_data, crypto_box2.encrypted_data);
    assert_ne!(crypto_box1.encrypted_data, crypto_box3.encrypted_data);
    assert_ne!(crypto_box2.encrypted_data, crypto_box4.encrypted_data);

    // Decrypt a few times in a few ways.
    let crypto_box_open1 = api_send.crypto_box_open_by_index(x25519_bob_index, x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box1)).await?;
    assert_eq!(&data, &crypto_box_open1.unwrap().data);
    let crypto_box_open2 = api_send.crypto_box_open_by_pub_key(x25519_bob_pub_key.clone(), x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box2)).await?;
    assert_eq!(&data, &crypto_box_open2.unwrap().data);
    let crypto_box_open3 = api_send2.crypto_box_open_by_index(x25519_bob_index, x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box3)).await?;
    assert_eq!(&data, &crypto_box_open3.unwrap().data);
    let crypto_box_open4 = api_send2.crypto_box_open_by_pub_key(x25519_bob_pub_key.clone(), x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box4.clone())).await?;
    assert_eq!(&data, &crypto_box_open4.unwrap().data);

    let (x25519_carol_index, x25519_carol_pub_key) = api_send.x25519_new_from_entropy().await?;
    assert_eq!(5, x25519_carol_index.0);

    // Show that decryption can fail.
    let crypto_box_open_carol = api_send2.crypto_box_open_by_pub_key(x25519_carol_pub_key, x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box4.clone())).await?;
    assert!(crypto_box_open_carol.is_none());

    // Ensure we didn't accidentally hang the ipc with an invalid decryption.
    let crypto_box_open5 = api_send2.crypto_box_open_by_pub_key(x25519_bob_pub_key.clone(), x25519_alice_pub_key.clone(), std::sync::Arc::new(crypto_box4)).await?;
    assert_eq!(&data, &crypto_box_open5.unwrap().data);

    drop(tmpdir);

    Ok(())
}
