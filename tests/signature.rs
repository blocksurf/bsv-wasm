#[cfg(test)]
mod tests {
    use bsv::*;
    use rayon::iter::{IntoParallelIterator, ParallelIterator};

    #[test]
    fn import_signature() {
        let sig_hex = "3044022075fc517e541bd54769c080b64397e32161c850f6c1b2b67a5c433affbb3e62770220729e85cc46ffab881065ec07694220e71d4df9b2b8c8fd12c3122cf3a5efbcf2";
        let sig = Signature::from_der(&hex::decode(sig_hex).unwrap()).unwrap();
        assert_eq!(sig.to_der_hex(), sig_hex)
    }

    #[test]
    fn import_signature_string() {
        let sig_hex = "3044022075fc517e541bd54769c080b64397e32161c850f6c1b2b67a5c433affbb3e62770220729e85cc46ffab881065ec07694220e71d4df9b2b8c8fd12c3122cf3a5efbcf2";
        let sig = Signature::from_hex_der(sig_hex).unwrap();
        assert_eq!(sig.to_der_hex(), sig_hex)
    }

    #[test]
    fn import_signature_with_sighash_string() {
        let sig_hex = "304402205ebadbf09cf9b9be17ee6f588e93f490a2db9ac5966f938255282cca9ca75fa602206c37c1842e1b48a177c195e34579be84826b7ad919cda6d803a5fc1d77551580c3";

        assert!(Signature::from_hex_der(sig_hex).is_ok())
    }

    // #[test]
    // #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    // fn der_signature_test_s_r() {
    //   let sig_hex = "3044022075fc517e541bd54769c080b64397e32161c850f6c1b2b67a5c433affbb3e62770220729e85cc46ffab881065ec07694220e71d4df9b2b8c8fd12c3122cf3a5efbcf2";
    //   let sig = Signature::from_hex_der(sig_hex.into()).unwrap();

    //   let verified = sig.verify();

    //   assert_eq!(sig.to_hex(), sig_hex)
    // }

    #[test]
    fn sign_message() {
        let wif = "L5EZftvrYaSudiozVRzTqLcHLNDoVn7H5HSfM9BAN6tMJX8oTWz6";

        let key = PrivateKey::from_wif(wif).unwrap();
        let message = b"Hello";

        let signature = key.sign_message(message).unwrap();
        let pub_key = PublicKey::from_private_key(&key);

        let is_verified = signature.verify_message(message, &pub_key);
        assert!(is_verified);
        assert_eq!(
            signature.to_der_hex(),
            "3045022100fab965a4dd445c990f46689f7acdc6e089128dc2d743457b350032d66336edb7022005f5684cc707b569120ef0442343998c95f6514c751251a91f82b1ec6a92da78".to_lowercase()
        )
    }

    #[test]
    fn recover_pub_key_from_signature_sha256() {
        let key = PrivateKey::from_wif("L4rGfRz3Q994Xns9wWti75K2CjxrCuzCqUAwN6yW7ia9nj4SDG32").unwrap();

        let message = b"Hello";

        let signature = key.sign_message(message).unwrap();
        let pub_key = PublicKey::from_private_key(&key);

        let is_verified = signature.verify_message(message, &pub_key);
        assert!(is_verified);

        let recovered_pub_key = signature.recover_public_key(message, SigningHash::Sha256).unwrap();
        assert_eq!(pub_key.to_hex().unwrap(), recovered_pub_key.to_hex().unwrap());
    }

    #[test]
    fn to_compact_test() {
        let key = PrivateKey::from_wif("L4rGfRz3Q994Xns9wWti75K2CjxrCuzCqUAwN6yW7ia9nj4SDG32").unwrap();

        let message = b"Hello";

        let signature = key.sign_message(message).unwrap();

        let compact_sig = signature.to_compact_bytes(None);
        let uncompacted_sig = Signature::from_compact_bytes(&compact_sig).unwrap();

        assert_eq!(uncompacted_sig.to_compact_bytes(None), signature.to_compact_bytes(None));
        assert_eq!(uncompacted_sig.to_der_bytes(), signature.to_der_bytes());
    }

    #[test]
    fn sign_with_k_test_par() {
        (0..2180i32).into_par_iter().for_each(|_i| {
            let private_key = PrivateKey::from_random();
            let public_key = PublicKey::from_private_key(&private_key);
            let ephemeral_key = PrivateKey::from_random();
            let message = PrivateKey::from_random().to_bytes();
            let signature = ECDSA::sign_with_k(&private_key, &ephemeral_key, &message, SigningHash::Sha256d).unwrap();
            let private_key_recovered = ECDSA::private_key_from_signature_k(&signature, &public_key, &ephemeral_key, &message, SigningHash::Sha256d).unwrap();
            assert!(private_key_recovered.to_bytes() == private_key.to_bytes());
            if _i % 10000 == 0 {
                println!("{}", _i);
            }
        });
    }

    #[test]
    fn sign_with_k_test() {
        // TODO: Handle for extremely low private key/ephemeral key
        let private_key = PrivateKey::from_random();
        // let private_key = PrivateKey::from_wif("5HpHagT65TZzG1PH3CSu63k8DbpvD8s5ip4nEB3kEsreAnchuDf").unwrap();
        let public_key = PublicKey::from_private_key(&private_key);
        let ephemeral_key = PrivateKey::from_random();
        // let ephemeral_key = PrivateKey::from_wif("5HpHagT65TZzG1PH3CSu63k8DbpvD8s5ip4nEB3kEsreAnchuDf").unwrap();
        let message = b"Hello";
        let signature = ECDSA::sign_with_k(&private_key, &ephemeral_key, message, SigningHash::Sha256d).unwrap();
        let private_key_recovered = ECDSA::private_key_from_signature_k(&signature, &public_key, &ephemeral_key, message, SigningHash::Sha256d).unwrap();
        assert!(private_key_recovered.to_bytes() == private_key.to_bytes());
    }

    #[test]
    /// Computes identical preimage & signature as moneybutton/bsv v1.
    ///
    /// Issue: In order for the values to match, the msg_scalar had to be flipped to big endian
    fn match_bsv_v1_preimage_and_sig() {
        let final_sig = "3045022100f81e04ae4e1be88c9eca52679e397c15cf40dd3678ff2328ef513af406edff3802201bc7116d947e934b375c6af4b90c2c029b9bf4a9dfcb3e3a28ed2723cd51b0cf41";

        let private_key = PrivateKey::from_wif("KzmbmFNo39aF85nxV24MqK3JCat3TYNUVJVaQdzgdSHLwuPFFyMt").unwrap();
        let p2pkh_script = private_key.to_public_key().unwrap().to_p2pkh_address().unwrap().get_locking_script().unwrap();

        let mut tx = Transaction::from_hex("010000000142c4eb085fdcf6c06faed60e1cbfcc852bb04801c70d5b499f122171b14f61af0000000000ffffffff021027000000000000fda6032097dfd76851bf465e8f715593b217714858bbe9570ff3bd5e33840a34e20ff0262102ba79df5f8ae7604a9830f03c7933028186aede0675a16f025dc4f8be8eec0382201008ce7480da41702918d1ec8e6849ba32b4d65b1e40dc669c31a1e6306b266c000014cdc1c584ca737579b470ab4407220d67a294774403df2418610079040065cd1d9f690079547a75537a537a537a5179537a75527a527a7575615579014161517957795779210ac407f0e4bd44bfc207355a778b046225a7068fc59ee7eda43ad905aadbffc800206c266b30e6a1319c66dc401e5bd6b432ba49688eecd118297041da8074ce081059795679615679aa0079610079517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e01007e81517a75615779567956795679567961537956795479577995939521414136d08c5ed2bf3ba048afe6dcaebafeffffffffffffffffffffffffffffff00517951796151795179970079009f63007952799367007968517a75517a75517a7561527a75517a517951795296a0630079527994527a75517a6853798277527982775379012080517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f517f7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e7c7e01205279947f7754537993527993013051797e527e54797e58797e527e53797e52797e57797e0079517a75517a75517a75517a75517a75517a75517a75517a75517a75517a75517a75517a75517a756100795779ac517a75517a75517a75517a75517a75517a75517a75517a75517a7561517a75517a756169557961007961007982775179517954947f75517958947f77517a75517a756161007901007e81517a7561517a7561040065cd1d9f6955796100796100798277517951790128947f755179012c947f77517a75517a756161007901007e81517a7561517a756105ffffffff009f69557961007961007982775179517954947f75517958947f77517a75517a756161007901007e81517a7561517a75615279a2695679a95179876957795779ac77777777777777772e5f0100000000001976a914cdc1c584ca737579b470ab4407220d67a294774488ac00000000").unwrap();
        let preimage = tx.sighash_preimage(bsv::SigHash::InputsOutputs, 0, &p2pkh_script, 99904).unwrap();
        assert_eq!(hex::encode(&preimage), "010000008c8dfd409fbe3bd76c6c074086b89b56e18c31899da98ccd7b05baf769a096ae3bb13029ce7b1f559ef5e747fcac439f1455a2ec7c5f09b72290795e7066504442c4eb085fdcf6c06faed60e1cbfcc852bb04801c70d5b499f122171b14f61af000000001976a914cdc1c584ca737579b470ab4407220d67a294774488ac4086010000000000ffffffff7bc9a0bf72f4ab98d03bd5b1f309976d9f2d30ff8e2c64ad2b3a13456015654b0000000041000000");

        let sig = ECDSA::sign_with_deterministic_k(&private_key, &preimage, bsv::SigningHash::Sha256d, false).unwrap();

        let mut der_bytes = sig.to_der_bytes();
        der_bytes.extend_from_slice(&[bsv::SigHash::InputsOutputs as u8]);

        assert_eq!(hex::encode(&der_bytes), final_sig);

        let tx_sig = tx
            .sign(
                &private_key,
                SigHash::InputsOutputs,
                0,
                &private_key.to_public_key().unwrap().to_p2pkh_address().unwrap().get_locking_script().unwrap(),
                99904,
            )
            .unwrap();

        assert_eq!(tx_sig.to_hex().unwrap(), final_sig);
    }
}
