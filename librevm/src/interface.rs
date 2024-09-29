use revm::{Context, Evm, EvmHandler};
use revm_primitives::{BlockEnv, EthereumWiring, ExecutionResult, HaltReason, SpecId, TxEnv};

use crate::{gstorage::GoStorage, ByteSliceView, Db, UnmanagedVector};
// byte slice view: golang data type
// unamangedvector: ffi safe vector data type compliants with rust's ownership and data types, for returning optional error value
pub const BLOCK: &str = "block";
pub const TRANSACTION: &str = "transaction";
enum ResultId {
    Success,
    Revert,
    Halt,
    Error,
}
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct evm_t {}

pub fn to_evm<'a>(ptr: *mut evm_t) -> Option<&'a mut Evm<'a, EthereumWiring<GoStorage<'a>, ()>>> {
    if ptr.is_null() {
        None
    } else {
        let evm = unsafe { &mut *(ptr as *mut Evm<'a, EthereumWiring<GoStorage<'a>, ()>>) };
        Some(evm)
    }
}

// initialize vm instance with handler
#[no_mangle]
pub extern "C" fn init_vm(// [] handler type -> validation / pre-execution / post-execution
    // GoApi -> api based on cosmos sdk
) -> *mut evm_t {
    let context = Context::default();
    let handler = EvmHandler::mainnet_with_spec(SpecId::CANCUN);
    // handler.post_execution = post_execution;
    // handler.pre_execution = pre_execution;
    let vm = Box::into_raw(Box::new(Evm::new(context, handler)));
    vm as *mut evm_t
}

#[no_mangle]
pub extern "C" fn release_vm(vm: *mut evm_t) {
    if !vm.is_null() {
        // this will free cache when it goes out of scope
        let _ = unsafe {
            Box::from_raw(vm as *mut Evm<'static, EthereumWiring<GoStorage<'static>, ()>>)
        };
    }
}

// VM initializer
#[no_mangle]
pub extern "C" fn execute_tx(
    vm_ptr: *mut evm_t,
    db: Db,               // -> Block Cache State from KVStore
    block: ByteSliceView, // -> block JSON Data
    tx: ByteSliceView,    // -> tx JSON Data
                          // errmsg: Option<&mut UnmanagedVector>
) -> UnmanagedVector {
    let evm = match to_evm(vm_ptr) {
        Some(vm) => vm,
        None => {
            panic!("Failed to get VM");
        }
    };
    let db = GoStorage::new(&db);
    evm.context = Context::new_with_db(db);
    set_evm_env(evm, block, tx);

    let result = evm.transact_commit();

    let data = match result {
        Ok(res) => handle_id(res),
        Err(_err) => {
            // let msg = err.to_string().into();
            // set_error(err, errmsg);
            vec![ResultId::Error as u8]
        }
    };
    UnmanagedVector::new(Some(data))
}

#[no_mangle]
pub extern "C" fn query(
    vm_ptr: *mut evm_t,
    db: Db,               // -> Block Cache State from KVStore
    block: ByteSliceView, // -> block JSON Data
    tx: ByteSliceView,    // -> tx JSON Data
                          // errmsg: Option<&mut UnmanagedVector>
) -> UnmanagedVector {
    let evm = match to_evm(vm_ptr) {
        Some(vm) => vm,
        None => {
            panic!("Failed to get VM");
        }
    };
    let db = GoStorage::new(&db);
    evm.context = Context::new_with_db(db);
    set_evm_env(evm, block, tx);
    // transact without state commit
    let result = evm.transact();
    let data = match result {
        Ok(res) => handle_id(res.result),
        Err(_err) => {
            // let msg = err.to_string().into();
            // set_error(err, errmsg);
            vec![ResultId::Error as u8]
        }
    };
    UnmanagedVector::new(Some(data))
}

fn handle_id(result: ExecutionResult<HaltReason>) -> Vec<u8> {
    let mut result = match result {
        ExecutionResult::Success {
            reason: _,
            gas_used: _,
            gas_refunded: _,
            logs: _,
            output: _,
        } => vec![ResultId::Success as u8],
        ExecutionResult::Revert {
            gas_used: _,
            output: _,
        } => vec![ResultId::Revert as u8],
        ExecutionResult::Halt {
            reason: _,
            gas_used: _,
        } => vec![ResultId::Halt as u8],
    };
    let mut data = serde_json::to_vec(&result).unwrap();
    result.append(&mut data);
    result
}

fn set_evm_env(
    evm: &mut Evm<'_, EthereumWiring<GoStorage<'_>, ()>>,
    block: ByteSliceView,
    tx: ByteSliceView,
) {
    let block: BlockEnv = serde_json::from_str(
        &String::from_utf8(
            block
                .read()
                .unwrap()
                //.ok_or_else(|| Error::unset_arg(BLOCK))?
                .to_vec(),
        )
        .unwrap(),
    )
    .unwrap();

    let tx: TxEnv = serde_json::from_str(
        &String::from_utf8(
            tx.read()
                .unwrap()
                //.ok_or_else(|| Error::unset_arg(TRANSACTION))?
                .to_vec(),
        )
        .unwrap(),
    )
    .unwrap();

    evm.context.evm.inner.env.block = block;
    evm.context.evm.inner.env.tx = tx;
}
