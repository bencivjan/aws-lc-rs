// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR ISC

use crate::{error::{Unspecified, KeyRejected}, ptr::LcPtr, ptr::{DetachableLcPtr, Pointer}, cipher};
use std::{os::raw::c_int};
use std::ptr::null_mut;
use aws_lc::{
    EVP_PKEY, NID_KYBER512_R3, EVP_PKEY_keygen, EVP_PKEY_CTX_new_id, EVP_PKEY_keygen_init,
    EVP_PKEY_KEM, EVP_PKEY_kem_new_raw_secret_key, EVP_PKEY_CTX_kem_set_params, EVP_PKEY_CTX_new,
    EVP_PKEY_encapsulate, EVP_PKEY_decapsulate
};
use zeroize::Zeroize;

const KYBER512_SECRETKEYBYTES: usize = 1632;
const KYBER512_PUBLICKEYBYTES: usize = 800;
const KYBER512_CIPHERTEXTBYTES: usize = 768;
const KYBER512_BYTES: usize = 32;

const PRIVATE_KEY_MAX_LEN: usize = KYBER512_SECRETKEYBYTES;
const PUBLIC_KEY_MAX_LEN: usize = KYBER512_PUBLICKEYBYTES;
const CIPHERTEXT_MAX_LEN: usize = KYBER512_CIPHERTEXTBYTES;
const SHARED_SECRET_MAX_LEN: usize = KYBER512_BYTES;


#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub enum Algorithm {
    KYBER512_R3,
}

impl Algorithm {
    #[inline]
    fn nid(&self) -> i32 {
        match self {
            Algorithm::KYBER512_R3 => NID_KYBER512_R3,
        }
    }
}

// KemPrivateKey
pub struct KemPrivateKey {
    algorithm: Algorithm,
    context: LcPtr<*mut EVP_PKEY>,
    // shared_secret: [u8; SHARED_SECRET_MAX_LEN]
}

impl KemPrivateKey {
    fn generate(alg: Algorithm) -> Result<Self, Unspecified> {
        match alg {
            Algorithm::KYBER512_R3 => unsafe {
                let kyber_key = kem_key_generate(alg.nid())?;
                Ok(KemPrivateKey {
                    algorithm: alg,
                    context: LcPtr::from(kyber_key),
                    // shared_secret: [0u8; SHARED_SECRET_MAX_LEN]
                })
            },
        }
    }

    fn algorithm(&self) -> &Algorithm {
        &self.algorithm
    }

    fn from_raw_bytes(alg: Algorithm, bytes: &[u8]) -> Result<Self, KeyRejected> {
        unsafe {
            let pkey = DetachableLcPtr::new(EVP_PKEY_kem_new_raw_secret_key(alg.nid(), bytes.as_ptr(), bytes.len()))?;
            Ok(KemPrivateKey {
                algorithm: alg,
                context: LcPtr::from(pkey),
                // shared_secret: [0u8; SHARED_SECRET_MAX_LEN]
            })
        }
    }

    fn compute_public_key(&self) -> Result<KemPublicKey, Unspecified> {
        // Could implement clone for LcPtr and call that here
        Ok(KemPublicKey{ algorithm: self.algorithm.clone(), context: LcPtr::new(*self.context)?,
        /*ciphertext: [0u8; CIPHERTEXT_MAX_LEN], shared_secret: [0u8; SHARED_SECRET_MAX_LEN]*/ })
    }

    fn decapsulate<F, R>(&self, ciphertext: &mut [u8], kdf: F) -> Result<R, Unspecified>
    where
        F: FnOnce(&[u8]) -> Result<R, Unspecified> {
        unsafe {
            let ctx = DetachableLcPtr::new(EVP_PKEY_CTX_new(*self.context, null_mut()))?;
            let mut shared_secret_len;
            match self.algorithm {
                Algorithm::KYBER512_R3 => {
                    // ciphertext_len = KYBER512_CIPHERTEXTBYTES;
                    shared_secret_len = KYBER512_SECRETKEYBYTES;
                }
            }
            let mut shared_secret = Vec::with_capacity(shared_secret_len);
            if EVP_PKEY_decapsulate(*ctx, shared_secret.as_mut_ptr(), &mut shared_secret_len,
                                    ciphertext.as_mut_ptr(), ciphertext.len()) != 1 {
                shared_secret.zeroize()
            }
            kdf(&shared_secret)
        }
    }
}

impl Into<[u8; PRIVATE_KEY_MAX_LEN]> for KemPrivateKey {
    fn into(self) -> [u8; PRIVATE_KEY_MAX_LEN] {
        [0u8; PRIVATE_KEY_MAX_LEN]
    }
}

/// An unparsed, possibly malformed, public key for key agreement.
// #[derive(Clone)]
pub struct KemPublicKey {
    algorithm: Algorithm,
    context: LcPtr<*mut EVP_PKEY>,
    // ciphertext: [u8; CIPHERTEXT_MAX_LEN],
    // shared_secret: [u8; SHARED_SECRET_MAX_LEN]
}

impl KemPublicKey {
    // fn from_raw_bytes(alg: Algorithm, bytes: &[u8]) -> Result<Self, KeyRejected> {
    //     Ok(KemPublicKey{ alg: &Algorithm::KYBER512_R3, context: bytes.try_into().map_err(|_e| KeyRejected::unexpected_error())? })
    // }

    fn encapsulate<F, R>(&self, kdf: F) -> Result<R, Unspecified>
    where
        F: FnOnce(&[u8], &[u8]) -> Result<R, Unspecified> {
        unsafe {
            let ctx = DetachableLcPtr::new(EVP_PKEY_CTX_new(*self.context, null_mut()))?;
            // get buffer lengths
            let mut ciphertext_len;
            let mut shared_secret_len;
            match self.algorithm {
                Algorithm::KYBER512_R3 => {
                    ciphertext_len = KYBER512_CIPHERTEXTBYTES;
                    shared_secret_len = KYBER512_SECRETKEYBYTES;
                }
            }
            let mut ciphertext = Vec::with_capacity(ciphertext_len);
            let mut shared_secret = Vec::with_capacity(shared_secret_len);
            EVP_PKEY_encapsulate(*ctx, ciphertext.as_mut_ptr(), &mut ciphertext_len,
                                    shared_secret.as_mut_ptr(), &mut shared_secret_len);
            kdf(&ciphertext, &shared_secret)
        }
    }
}

impl Into<[u8; PUBLIC_KEY_MAX_LEN]> for KemPublicKey {
    fn into(self) -> [u8; PUBLIC_KEY_MAX_LEN] {
        [0; PUBLIC_KEY_MAX_LEN]
    }
}

// Returns a DetachableLcPtr to an EVP_PKEY
#[inline]
unsafe fn kem_key_generate(
    nid: c_int,
) -> Result<DetachableLcPtr<*mut EVP_PKEY>, Unspecified> {
    let ctx = DetachableLcPtr::new(EVP_PKEY_CTX_new_id(EVP_PKEY_KEM, null_mut()))?;
    let mut key_raw = null_mut();
    if 1 != EVP_PKEY_keygen_init(*ctx) ||
       1 != EVP_PKEY_CTX_kem_set_params(*ctx, nid) ||
       1 != EVP_PKEY_keygen(*ctx, &mut key_raw) {
        // We don't have the key wrapped with LcPtr yet, so explicitly free it
        key_raw.free();
        return Err(Unspecified);
    }
    Ok(DetachableLcPtr::new(key_raw)?)
}

#[cfg(test)]
mod tests {
    use crate::{key_transport, rand, test, test_file};

    use super::KemPrivateKey;

    #[test]
    fn test_agreement_kyber512() {
        let priv_key = KemPrivateKey::generate(key_transport::Algorithm::KYBER512_R3).unwrap();
        assert_eq!(priv_key.algorithm(), &key_transport::Algorithm::KYBER512_R3);

        let pub_key = priv_key.compute_public_key().unwrap();

        let mut ciphertext: Vec<u8> = vec![];
        let mut alice_shared_secret: Vec<u8> = vec![];

        let alice_result = pub_key.encapsulate(|ct, ss| {
            ciphertext.extend_from_slice(ct);
            alice_shared_secret.extend_from_slice(ss);
            Ok(())
        });
        assert_eq!(alice_result, Ok(()));

        let mut bob_shared_secret: Vec<u8> = vec![];

        let bob_result = priv_key.decapsulate(&mut ciphertext, |ss| {
            bob_shared_secret.extend_from_slice(ss);
            Ok(())
        });
        assert_eq!(bob_result, Ok(()));

        assert_eq!(alice_shared_secret, bob_shared_secret);
    }

    #[test]
    fn test_serialized_agreement_kyber512() {

    }
}
