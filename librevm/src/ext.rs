use std::{env, fs::File, io::Write, sync::Arc};

use alloy_primitives::B256;
use revm::{handler::register::EvmHandler, Database};
use revmc::{eyre::Result, EvmCompilerFn};

use crate::{
    aot::{KeyPrefix, QueryKey, QueryKeySlice, SledDB},
    utils::ivec_to_i32,
    SLED_DB,
};

pub struct ExternalContext {}

impl ExternalContext {
    pub fn new() -> Self {
        Self {}
    }

    fn get_function(
        &self,
        bytecode_hash: B256,
    ) -> Result<Option<(EvmCompilerFn, libloading::Library)>> {
        let sled_db = SLED_DB.get_or_init(|| Arc::new(SledDB::<QueryKeySlice>::init()));
        let key = QueryKey::with_prefix(bytecode_hash, KeyPrefix::SO);

        if let Some(so_bytes) = sled_db.get(*key.as_inner()).unwrap_or(None) {
            let temp_dir = env::temp_dir();
            let temp_file_path = temp_dir.join("a.so");

            let mut file = File::create(&temp_file_path)?;
            file.write_all(&so_bytes).unwrap();

            let lib;
            let f = {
                lib = unsafe { libloading::Library::new(&temp_file_path) }.unwrap();
                let f: libloading::Symbol<'_, revmc::EvmCompilerFn> =
                    unsafe { lib.get(bytecode_hash.to_string().as_ref()).unwrap() };
                *f
            };

            return Ok(Some((f, lib)));
        }

        Ok(None)
    }

    fn update_bytecode_reference(&self, bytecode_hash: B256) -> Result<()> {
        let sled_db = SLED_DB.get_or_init(|| Arc::new(SledDB::<QueryKeySlice>::init()));
        let key = QueryKey::with_prefix(bytecode_hash, KeyPrefix::Count);

        let count_bytes = sled_db.get(*key.as_inner()).unwrap_or(None);
        let count = count_bytes.and_then(|v| ivec_to_i32(&v)).unwrap_or(0) + 1;

        sled_db
            .put(*key.as_inner(), &count.to_be_bytes(), true)
            .unwrap();

        Ok(())
    }
}

// This `+ 'static` bound is only necessary here because of an internal cfg feature.
pub fn register_handler<DB: Database>(handler: &mut EvmHandler<'_, ExternalContext, DB>) {
    let prev = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, tables, context| {
        let interpreter = frame.interpreter_mut();
        let bytecode_hash = interpreter.contract.hash.unwrap_or_default();

        context
            .external
            .update_bytecode_reference(bytecode_hash)
            .unwrap();

        if let Some((f, _lib)) = context.external.get_function(bytecode_hash).unwrap() {
            println!("EvmCompilerFn from hash: {bytecode_hash:#?}");
            Ok(unsafe { f.call_with_interpreter_and_memory(interpreter, memory, context) })
        } else {
            prev(frame, memory, tables, context)
        }
    });
}
