use std::{io::Read, path::Path, sync::Arc};

use alloy_primitives::{Address, B256};
use revm::{handler::register::EvmHandler, Database};
use revmc::{eyre::Result, EvmCompilerFn};

use crate::{
    jit::{KeyPrefix, QueryKey, QueryKeySlice, SledDB, JIT_OUT_PATH},
    SLED_DB,
};

pub struct ExternalContext {}

impl ExternalContext {
    pub fn new() -> Self {
        Self {}
    }

    fn get_function(&self, address: Address) -> Option<EvmCompilerFn> {
        // TODO: Restrain from initializing db every get function call
        let sled_db = SLED_DB.get_or_init(|| Arc::new(SledDB::<QueryKeySlice>::init()));

        let mut padded = [0u8; 32];
        padded[..20].copy_from_slice(address.as_slice());
        let label_key = QueryKey::with_prefix(B256::from_slice(&padded), KeyPrefix::Label);

        println!("Hey?, {:#?}", padded);

        let maybe_label = sled_db.get(*label_key.as_inner()).unwrap_or(None);
        if let Some(label) = maybe_label {
            let fn_label = String::from_utf8(label.to_vec()).unwrap();

            let lib;
            let f = {
                let jit_out_path = Path::new(JIT_OUT_PATH);

                let so_path = jit_out_path.join(&fn_label).join("a.so");

                lib = unsafe { libloading::Library::new(so_path) }
                    .expect("Should've loaded linked library");
                let f: libloading::Symbol<'_, revmc::EvmCompilerFn> =
                    unsafe { lib.get(fn_label.as_bytes()).expect("Should've got library") };
                println!("f: {f:#?}");
                *f
            };

            return Some(f);
        }
        //
        None
    }

    fn update_bytecode_reference(&self, address: Address) -> Result<()> {
        // TODO: Restrain from initializing db every inc call
        let sled_db = SLED_DB.get_or_init(|| Arc::new(SledDB::<QueryKeySlice>::init()));
        let mut padded = [0u8; 32];
        padded[..20].copy_from_slice(address.as_slice());
        let count_key = QueryKey::with_prefix(B256::from_slice(&padded), KeyPrefix::Count);

        let count = sled_db.get(*count_key.as_inner()).unwrap_or(None);
        let new_count = count.as_ref().map_or(1, |v| {
            let bytes: [u8; 4] = v.to_vec().as_slice().try_into().unwrap_or([0, 0, 0, 0]);
            i32::from_be_bytes(bytes) + 1
        });

        sled_db
            .put(*count_key.as_inner(), &new_count.to_be_bytes(), true)
            .unwrap();

        Ok(())
    }

    fn update_bytecode(&self, bytecode: &[u8], address: Address) -> Result<()> {
        let sled_db = SLED_DB.get_or_init(|| Arc::new(SledDB::<QueryKeySlice>::init()));
        println!("Who?, {:#?}", address);
        // 9 cause 10 can cause unexpected behavior
        let mut padded = [0u8; 32];
        padded[..20].copy_from_slice(address.as_slice());
        let label_key = QueryKey::with_prefix(B256::from_slice(&padded), KeyPrefix::Label);
        if let None = sled_db.get(*label_key.as_inner()).unwrap_or(None) {
            let bytecode_key =
                QueryKey::with_prefix(B256::from_slice(&padded), KeyPrefix::Bytecode);

            sled_db
                .put(*bytecode_key.as_inner(), bytecode, true)
                .unwrap();
        }
        Ok(())
    }
}

// This `+ 'static` bound is only necessary here because of an internal cfg feature.
pub fn register_handler<DB: Database>(handler: &mut EvmHandler<'_, ExternalContext, DB>) {
    let prev = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, tables, context| {
        let interpreter = frame.interpreter_mut();
        let bytecode_hash = interpreter.contract.hash.unwrap_or_default();
        let bytecode = interpreter.contract.bytecode.bytes_slice();
        let contract_address = interpreter.contract.target_address;

        println!("Checking for bytecode hash: {:#?}\n", bytecode_hash);
        println!("Checking for bytecode: {:#?}\n\n", bytecode);
        println!("Contract address: {:#?}\n", contract_address);

        match is_create_frame(*bytecode_hash) {
            true => context
                .external
                .update_bytecode(bytecode, contract_address)
                .expect("Update bytecode failed"),

            false => {
                context
                    .external
                    .update_bytecode_reference(contract_address)
                    .expect("Update bytecode hash failed");

                if let Some(f) = context.external.get_function(contract_address) {
                    println!("Calling extern function on hash: {f:#?}");
                    return Ok(unsafe {
                        f.call_with_interpreter_and_memory(interpreter, memory, context)
                    });
                }
            }
        };

        prev(frame, memory, tables, context)
    });
}

fn is_create_frame(bytecode_hash: [u8; 32]) -> bool {
    bytecode_hash.iter().all(|&byte| byte == 0)
}
