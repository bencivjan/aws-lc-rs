// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR ISC

//! Key Transport: KEMs, including support for Kyber.
//!
//! # Example
//!
//! Note that this example uses Kyber-512, but other algorithms can be used
//! exactly the same way, just substituting
//! `KemAlgorithm::<desired_algorithm_here>` for `KemAlgorithm::KYBER512_R3`.
//!
//! ```
//! use aws_lc_rs::key_transport::{KemAlgorithm, KemPrivateKey, KemPublicKey};
//!
//! let priv_key = KemPrivateKey::generate(KemAlgorithm::KYBER512_R3)?;
//!
//! // Generate private key bytes to possibly save for later decapsulation
//! let privkey_raw_bytes = priv_key.as_ref();
//!
//! let pub_key = priv_key.compute_public_key()?;
//!
//! // Get the public key bytes to send to bob through some encoded
//! // protocol message.
//! let pub_key_bytes = pub_key.as_ref();
//!
//! let mut ciphertext: Vec<u8> = vec![];
//!
//! let retrieved_pub_key = KemPublicKey::new(KemAlgorithm::KYBER512_R3, pub_key_bytes)?;
//! let bob_result = retrieved_pub_key.encapsulate(|ct, ss| {
//!     ciphertext.extend_from_slice(ct);
//!     // In a real application, we'd apply a KDF to the shared secret and the
//!     // public keys (as recommended in RFC 7748) and then derive session
//!     // keys from the result. We omit all that here.
//!     Ok(())
//! });
//!
//! // Retrieve private key from stored raw bytes
//! let retrieved_priv_key = KemPrivateKey::new(KemAlgorithm::KYBER512_R3, privkey_raw_bytes)?;
//!
//! let alice_result = retrieved_priv_key.decapsulate(&mut ciphertext, |ss| {
//!     // In a real application, we'd apply a KDF to the shared secret and the
//!     // public keys (as recommended in RFC 7748) and then derive session
//!     // keys from the result. We omit all that here.
//!     Ok(())
//! });
//!
//! # Ok::<(), aws_lc_rs::error::Unspecified>(())
//! ```
use crate::{
    error::{KeyRejected, Unspecified},
    ptr::LcPtr,
    ptr::Pointer,
};
use aws_lc::{
    EVP_PKEY_CTX_kem_set_params, EVP_PKEY_CTX_new, EVP_PKEY_CTX_new_id, EVP_PKEY_decapsulate,
    EVP_PKEY_encapsulate, EVP_PKEY_get_raw_private_key, EVP_PKEY_get_raw_public_key,
    EVP_PKEY_kem_new_raw_public_key, EVP_PKEY_kem_new_raw_secret_key, EVP_PKEY_keygen,
    EVP_PKEY_keygen_init, EVP_PKEY, EVP_PKEY_KEM, NID_KYBER512_R3,
};
use std::os::raw::c_int;
use std::ptr::null_mut;
use zeroize::Zeroize;

const KYBER512_SECRETKEYBYTES: usize = 1632;
const KYBER512_PUBLICKEYBYTES: usize = 800;
const KYBER512_CIPHERTEXTBYTES: usize = 768;
const KYBER512_BYTES: usize = 32;

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
/// A selection of algorithms to be used as KEMs.
pub enum KemAlgorithm {
    /// NIST Round 3 iteration of the Kyber-512 algorithm
    KYBER512_R3,
}

impl KemAlgorithm {
    #[inline]
    fn nid(&self) -> i32 {
        match self {
            KemAlgorithm::KYBER512_R3 => NID_KYBER512_R3,
        }
    }

    #[inline]
    fn get_pubkey_len(&self) -> usize {
        match self {
            KemAlgorithm::KYBER512_R3 => KYBER512_PUBLICKEYBYTES,
        }
    }

    #[inline]
    fn get_privkey_len(&self) -> usize {
        match self {
            KemAlgorithm::KYBER512_R3 => KYBER512_SECRETKEYBYTES,
        }
    }
}
/// A serializable private key usable with KEMs. This can be randomly generated
/// or constructed from raw bytes.
#[derive(Debug)]
pub struct KemPrivateKey {
    algorithm: KemAlgorithm,
    context: LcPtr<*mut EVP_PKEY>,
    priv_key: Vec<u8>,
}

impl KemPrivateKey {
    /// Generate a new KEM private key for the given algorithm.
    ///
    /// # Errors
    /// `error::Unspecified` when operation fails due to internal error.
    ///
    pub fn generate(alg: KemAlgorithm) -> Result<Self, Unspecified> {
        match alg {
            KemAlgorithm::KYBER512_R3 => unsafe {
                let kyber_key = kem_key_generate(alg.nid())?;
                let mut priv_key_bytes = vec![0u8; alg.get_privkey_len()];
                if 1 != EVP_PKEY_get_raw_private_key(
                    *kyber_key,
                    priv_key_bytes.as_mut_ptr(),
                    &mut alg.get_privkey_len(),
                ) {
                    return Err(Unspecified);
                }
                Ok(KemPrivateKey {
                    algorithm: alg,
                    context: kyber_key,
                    priv_key: priv_key_bytes,
                })
            },
        }
    }

    /// Return the algorithm associated with the given KEM private key.
    #[must_use]
    pub fn algorithm(&self) -> &KemAlgorithm {
        &self.algorithm
    }

    /// Computes the KEM public key from the KEM private key
    ///
    /// # Errors
    /// `error::Unspecified` when operation fails due to internal error.
    ///
    pub fn compute_public_key(&self) -> Result<KemPublicKey, Unspecified> {
        let mut pubkey_bytes = vec![0u8; self.algorithm.get_pubkey_len()];
        let mut pubkey_len;
        let pubkey_ctx_copy;
        match self.algorithm {
            KemAlgorithm::KYBER512_R3 => {
                pubkey_len = KYBER512_PUBLICKEYBYTES;
            }
        }
        unsafe {
            if 1 != EVP_PKEY_get_raw_public_key(
                *self.context,
                pubkey_bytes.as_mut_ptr(),
                &mut pubkey_len,
            ) {
                return Err(Unspecified);
            }
            pubkey_ctx_copy = EVP_PKEY_kem_new_raw_public_key(
                self.algorithm.nid(),
                pubkey_bytes.as_mut_ptr(),
                pubkey_len,
            );
            if pubkey_ctx_copy.is_null() {
                return Err(Unspecified);
            }
        }
        Ok(KemPublicKey {
            algorithm: self.algorithm.clone(),
            context: LcPtr::new(pubkey_ctx_copy)?,
            pub_key: pubkey_bytes,
        })
    }

    /// Performs the decapsulate operation using the current KEM private key on the given ciphertext.
    ///
    /// `ciphertext` is the ciphertext generated by the encapsulate operation using the KEM public key
    /// associated with the current KEM private key.
    ///
    /// After the decapsulation is finished, `decapsulate` calls `kdf` with the raw shared secret
    /// from the operation and then returns what `kdf` returns.
    ///
    /// # Errors
    /// `error::Unspecified` when operation fails due to internal error.
    ///
    pub fn decapsulate<F, R>(&self, ciphertext: &mut [u8], kdf: F) -> Result<R, Unspecified>
    where
        F: FnOnce(&[u8]) -> Result<R, Unspecified>,
    {
        unsafe {
            let ctx = LcPtr::new(EVP_PKEY_CTX_new(*self.context, null_mut()))?;
            let mut shared_secret_len;
            match self.algorithm {
                KemAlgorithm::KYBER512_R3 => {
                    shared_secret_len = KYBER512_BYTES;
                }
            }
            let mut shared_secret: Vec<u8> = vec![0u8; shared_secret_len];
            if EVP_PKEY_decapsulate(
                *ctx,
                shared_secret.as_mut_ptr(),
                &mut shared_secret_len,
                ciphertext.as_mut_ptr(),
                ciphertext.len(),
            ) != 1
            {
                shared_secret.zeroize();
                return Err(Unspecified);
            }
            kdf(&shared_secret)
        }
    }

    /// Creates a new KEM private key from raw bytes
    ///
    /// `alg` is the `KemAlgorithm` to be associated with the generated `KemPrivateKey`
    ///
    /// `bytes` is a slice of raw bytes representing a `KemPrivateKey`
    ///
    /// # Errors
    /// `error::KeyRejected` when operation fails during key creation.
    ///
    pub fn new(alg: KemAlgorithm, bytes: &[u8]) -> Result<Self, KeyRejected> {
        unsafe {
            let pkey = LcPtr::new(EVP_PKEY_kem_new_raw_secret_key(
                alg.nid(),
                bytes.as_ptr(),
                bytes.len(),
            ))?;
            Ok(KemPrivateKey {
                algorithm: alg,
                context: LcPtr::from(pkey),
                priv_key: bytes.to_owned(),
            })
        }
    }
}

impl AsRef<[u8]> for KemPrivateKey {
    fn as_ref(&self) -> &[u8] {
        &self.priv_key
    }
}

/// A serializable public key usable with KEMS. This can be constructed
/// from a `KemPrivateKey` or constructed from raw bytes.
#[derive(Debug)]
pub struct KemPublicKey {
    algorithm: KemAlgorithm,
    context: LcPtr<*mut EVP_PKEY>,
    pub_key: Vec<u8>,
}

impl KemPublicKey {
    /// Return the algorithm associated with the given KEM public key.
    #[must_use]
    pub fn algorithm(&self) -> &KemAlgorithm {
        &self.algorithm
    }

    /// Performs the encapsulate operation using the current KEM public key, generating a ciphertext
    /// and associated shared secret.
    ///
    /// After the decapsulation is finished, `decapsulate` calls `kdf` with the ciphertext and raw shared secret
    /// from the operation and then returns what `kdf` returns.
    ///
    /// # Errors
    /// `error::Unspecified` when operation fails due to internal error.
    ///
    pub fn encapsulate<F, R>(&self, kdf: F) -> Result<R, Unspecified>
    where
        F: FnOnce(&[u8], &[u8]) -> Result<R, Unspecified>,
    {
        unsafe {
            let ctx = LcPtr::new(EVP_PKEY_CTX_new(*self.context, null_mut()))?;
            let mut ciphertext_len;
            let mut shared_secret_len;
            match self.algorithm {
                KemAlgorithm::KYBER512_R3 => {
                    ciphertext_len = KYBER512_CIPHERTEXTBYTES;
                    shared_secret_len = KYBER512_BYTES;
                }
            }

            let mut ciphertext: Vec<u8> = vec![0u8; ciphertext_len];
            let mut shared_secret: Vec<u8> = vec![0u8; shared_secret_len];

            if EVP_PKEY_encapsulate(
                *ctx,
                ciphertext.as_mut_ptr(),
                &mut ciphertext_len,
                shared_secret.as_mut_ptr(),
                &mut shared_secret_len,
            ) != 1
            {
                ciphertext.zeroize();
                shared_secret.zeroize();
                return Err(Unspecified);
            }

            #[cfg(feature = "debug-kem")]
            {
                println!("Ciphertext length: {}", ciphertext_len);
                println!("Shared secret length: {}", shared_secret_len);
                println!("Ciphertext: {:02x?}", ciphertext);
                println!("Shared Secret: {:02x?}", shared_secret);
            }

            kdf(&ciphertext, &shared_secret)
        }
    }

    /// Creates a new KEM public key from raw bytes
    ///
    /// `alg` is the `KemAlgorithm` to be associated with the generated `KemPublicKey`
    ///
    /// `bytes` is a slice of raw bytes representing a `KemPublicKey`
    ///
    /// # Errors
    /// `error::KeyRejected` when operation fails during key creation.
    ///
    pub fn new(alg: KemAlgorithm, bytes: &[u8]) -> Result<Self, KeyRejected> {
        unsafe {
            let pubkey_ctx =
                EVP_PKEY_kem_new_raw_public_key(alg.nid(), bytes.as_ptr(), alg.get_pubkey_len());
            if pubkey_ctx.is_null() {
                return Err(KeyRejected::unexpected_error());
            }
            Ok(KemPublicKey {
                algorithm: alg,
                context: LcPtr::new(pubkey_ctx)?,
                pub_key: bytes.to_owned(),
            })
        }
    }
}

impl AsRef<[u8]> for KemPublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.pub_key
    }
}

// Returns an LcPtr to an EVP_PKEY
#[inline]
unsafe fn kem_key_generate(nid: c_int) -> Result<LcPtr<*mut EVP_PKEY>, Unspecified> {
    let ctx = LcPtr::new(EVP_PKEY_CTX_new_id(EVP_PKEY_KEM, null_mut()))?;
    let mut key_raw: *mut EVP_PKEY = null_mut();
    if 1 != EVP_PKEY_keygen_init(*ctx)
        || 1 != EVP_PKEY_CTX_kem_set_params(*ctx, nid)
        || 1 != EVP_PKEY_keygen(*ctx, &mut key_raw)
    {
        // We don't have the key wrapped with LcPtr yet, so explicitly free it
        key_raw.free();
        return Err(Unspecified);
    }

    Ok(LcPtr::new(key_raw)?)
}

#[cfg(test)]
mod tests {
    use crate::key_transport::{KemAlgorithm, KemPrivateKey, KemPublicKey};

    #[test]
    fn test_kem_privkey_serialize() {
        let priv_key = KemPrivateKey::generate(KemAlgorithm::KYBER512_R3).unwrap();
        assert_eq!(priv_key.algorithm(), &KemAlgorithm::KYBER512_R3);

        let privkey_raw_bytes = priv_key.as_ref();
        let priv_key_from_bytes =
            KemPrivateKey::new(KemAlgorithm::KYBER512_R3, privkey_raw_bytes).unwrap();

        assert_eq!(
            priv_key.as_ref(),
            priv_key_from_bytes.as_ref()
        );
        assert_eq!(priv_key.algorithm(), priv_key_from_bytes.algorithm());
    }

    #[test]
    fn test_kem_pubkey_serialize() {
        let priv_key = KemPrivateKey::generate(KemAlgorithm::KYBER512_R3).unwrap();
        assert_eq!(priv_key.algorithm(), &KemAlgorithm::KYBER512_R3);

        let pub_key = priv_key.compute_public_key().unwrap();

        let pubkey_raw_bytes = pub_key.as_ref();
        let pub_key_from_bytes =
            KemPublicKey::new(KemAlgorithm::KYBER512_R3, pubkey_raw_bytes).unwrap();

        assert_eq!(pub_key.as_ref(), pub_key_from_bytes.as_ref());
        assert_eq!(pub_key.algorithm(), pub_key_from_bytes.algorithm());
    }
}
