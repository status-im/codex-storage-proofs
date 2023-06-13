use ruint::aliases::U256;

use crate::storage_proofs::StorageProofs;
use std::str;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Buffer {
    pub data: *const u8,
    pub len: usize,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProofCtx {
    pub proof: Buffer,
    pub public_inputs: Buffer,
}

impl ProofCtx {
    pub fn new(proof: &[u8], public_inputs: &[u8]) -> Self {
        Self {
            proof: Buffer {
                data: proof.as_ptr(),
                len: proof.len(),
            },
            public_inputs: Buffer {
                data: public_inputs.as_ptr(),
                len: public_inputs.len(),
            },
        }
    }
}

/// # Safety
///
/// Construct a StorageProofs object
#[no_mangle]
pub unsafe extern "C" fn init(
    r1cs: *const &Buffer,
    wasm: *const &Buffer,
    zkey: *const &Buffer,
) -> *mut StorageProofs {
    let r1cs = {
        if r1cs.is_null() {
            return std::ptr::null_mut();
        }

        let slice = std::slice::from_raw_parts((*r1cs).data, (*r1cs).len);
        str::from_utf8(slice).unwrap().to_string()
    };

    let wasm = {
        if wasm.is_null() {
            return std::ptr::null_mut();
        }

        let slice = std::slice::from_raw_parts((*wasm).data, (*wasm).len);
        str::from_utf8(slice).unwrap().to_string()
    };

    let zkey = {
        if !zkey.is_null() {
            let slice = std::slice::from_raw_parts((*zkey).data, (*zkey).len);
            Some(str::from_utf8(slice).unwrap().to_string())
        } else {
            None
        }
    };

    Box::into_raw(Box::new(StorageProofs::new(wasm, r1cs, zkey)))
}

/// # Safety
///
/// Use after constructing a StorageProofs object with init
#[no_mangle]
pub unsafe extern "C" fn prove(
    prover_ptr: *mut StorageProofs,
    chunks: *const Buffer,
    siblings: *const Buffer,
    hashes: *const Buffer,
    path: *const i32,
    path_len: usize,
    pubkey: *const Buffer,
    root: *const Buffer,
    salt: *const Buffer,
) -> *mut ProofCtx {
    let chunks = {
        let slice = std::slice::from_raw_parts((*chunks).data, (*chunks).len);
        slice
            .chunks(U256::BYTES)
            .map(|c| U256::try_from_le_slice(c).unwrap())
            .collect::<Vec<U256>>()
    };

    let siblings = {
        let slice = std::slice::from_raw_parts((*siblings).data, (*siblings).len);
        slice
            .chunks(U256::BYTES)
            .map(|c| U256::try_from_le_slice(c).unwrap())
            .collect::<Vec<U256>>()
    };

    let hashes = {
        let slice = std::slice::from_raw_parts((*hashes).data, (*hashes).len);
        slice
            .chunks(U256::BYTES)
            .map(|c| U256::try_from_le_slice(c).unwrap())
            .collect::<Vec<U256>>()
    };

    let path = {
        let slice = std::slice::from_raw_parts(path, path_len);
        slice.to_vec()
    };

    let pubkey =
        U256::try_from_le_slice(std::slice::from_raw_parts((*pubkey).data, (*pubkey).len)).unwrap();

    let root =
        U256::try_from_le_slice(std::slice::from_raw_parts((*root).data, (*root).len)).unwrap();

    let salt =
        U256::try_from_le_slice(std::slice::from_raw_parts((*salt).data, (*salt).len)).unwrap();

    let proof_bytes = &mut Vec::new();
    let public_inputs_bytes = &mut Vec::new();

    let mut _prover = &mut *prover_ptr;
    _prover
        .prove(
            chunks.as_slice(),
            siblings.as_slice(),
            hashes.as_slice(),
            path.as_slice(),
            root,
            salt,
            proof_bytes,
            public_inputs_bytes,
        )
        .unwrap();

    Box::into_raw(Box::new(ProofCtx::new(proof_bytes, public_inputs_bytes)))
}

#[no_mangle]
/// # Safety
///
/// Should be called on a valid proof and public inputs previously generated by prove
pub unsafe extern "C" fn verify(
    prover_ptr: *mut StorageProofs,
    proof: *const Buffer,
    public_inputs: *const Buffer,
) -> bool {
    let proof = std::slice::from_raw_parts((*proof).data, (*proof).len);
    let public_inputs = std::slice::from_raw_parts((*public_inputs).data, (*public_inputs).len);
    let mut _prover = &mut *prover_ptr;
    _prover.verify(proof, public_inputs).is_ok()
}

/// # Safety
///
/// Use on a valid pointer to StorageProofs or panics
#[no_mangle]
pub unsafe extern "C" fn free_prover(prover: *mut StorageProofs) {
    if prover.is_null() {
        return;
    }

    unsafe { drop(Box::from_raw(prover)) }
}

/// # Safety
///
/// Use on a valid pointer to ProofCtx or panics
#[no_mangle]
pub unsafe extern "C" fn free_proof_ctx(ctx: *mut ProofCtx) {
    if ctx.is_null() {
        return;
    }

    drop(Box::from_raw(ctx))
}

#[cfg(test)]
mod tests {
    use ark_std::rand::{distributions::Alphanumeric, rngs::ThreadRng, Rng};
    use rs_poseidon::poseidon::hash;
    use ruint::aliases::U256;

    use crate::{
        circuit_tests::utils::{digest, treehash},
    };

    use super::{init, prove, Buffer};

    #[test]
    fn test_storer_ffi() {
        // generate a tuple of (preimages, hash), where preimages is a vector of 256 U256s
        // and hash is the hash of each vector generated using the digest function
        let data = (0..4)
            .map(|_| {
                let rng = ThreadRng::default();
                let preimages: Vec<U256> = rng
                    .sample_iter(Alphanumeric)
                    .take(256)
                    .map(|c| U256::from(c))
                    .collect();
                let hash = digest(&preimages, Some(16));
                (preimages, hash)
            })
            .collect::<Vec<(Vec<U256>, U256)>>();

        let chunks: Vec<u8> = data
            .iter()
            .map(|c| {
                c.0.iter()
                    .map(|c| c.to_le_bytes_vec())
                    .flatten()
                    .collect::<Vec<u8>>()
            })
            .flatten()
            .collect();

        let hashes: Vec<U256> = data.iter().map(|c| c.1).collect();
        let hashes_slice: Vec<u8> = hashes.iter().map(|c| c.to_le_bytes_vec()).flatten().collect();

        let path = [0, 1, 2, 3];
        let parent_hash_l = hash(&[hashes[0], hashes[1]]);
        let parent_hash_r = hash(&[hashes[2], hashes[3]]);

        let sibling_hashes = &[
            hashes[1],
            parent_hash_r,
            hashes[0],
            parent_hash_r,
            hashes[3],
            parent_hash_l,
            hashes[2],
            parent_hash_l,
        ];

        let siblings: Vec<u8> = sibling_hashes
            .iter()
            .map(|c| c.to_le_bytes_vec())
            .flatten()
            .collect();

        let root = treehash(hashes.as_slice());
        let chunks_buff = Buffer {
            data: chunks.as_ptr() as *const u8,
            len: chunks.len(),
        };

        let siblings_buff = Buffer {
            data: siblings.as_ptr() as *const u8,
            len: siblings.len(),
        };

        let hashes_buff = Buffer {
            data: hashes_slice.as_ptr() as *const u8,
            len: hashes_slice.len(),
        };

        let root_bytes: [u8; U256::BYTES] = root.to_le_bytes();
        let root_buff = Buffer {
            data: root_bytes.as_ptr() as *const u8,
            len: root_bytes.len(),
        };

        let r1cs_path = "src/circuit_tests/artifacts/storer-test.r1cs";
        let wasm_path = "src/circuit_tests/artifacts/storer-test_js/storer-test.wasm";

        let r1cs = &Buffer {
            data: r1cs_path.as_ptr(),
            len: r1cs_path.len(),
        };

        let wasm = &Buffer {
            data: wasm_path.as_ptr(),
            len: wasm_path.len(),
        };

        let prover_ptr = unsafe { init(&r1cs, &wasm, std::ptr::null()) };
        let prove_ctx = unsafe {
            prove(
                prover_ptr,
                &chunks_buff as *const Buffer,
                &siblings_buff as *const Buffer,
                &hashes_buff as *const Buffer,
                &path as *const i32,
                path.len(),
                &root_buff as *const Buffer, // root
                &root_buff as *const Buffer, // pubkey
                &root_buff as *const Buffer, // salt/block hash
            )
        };

        assert!(prove_ctx.is_null() == false);
    }
}
