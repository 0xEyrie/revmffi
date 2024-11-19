use crate::{
    aot::{Compiler, QueryKeySlice, SledDB},
    db::Db,
    error::set_error,
    ext::{register_handler, ExternalContext},
    gstorage::GoStorage,
    memory::{ByteSliceView, UnmanagedVector},
    utils::{build_flat_buffer, set_evm_env},
};
use once_cell::sync::OnceCell;
use revm::{primitives::SpecId, Evm, EvmBuilder};
use std::sync::{Arc, RwLock};

pub static SLED_DB: OnceCell<Arc<RwLock<SledDB<QueryKeySlice>>>> = OnceCell::new();

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct compiler_t {}

pub fn to_compiler<'a>(ptr: *mut compiler_t) -> Option<&'a mut Compiler> {
    if ptr.is_null() {
        None
    } else {
        let compiler = unsafe { &mut *(ptr as *mut Compiler) };
        Some(compiler)
    }
}

#[tokio::main]
#[no_mangle]
pub async extern "C" fn init_compiler(interval: u64) -> *mut compiler_t {
    let sled_db = SLED_DB.get_or_init(|| Arc::new(RwLock::new(SledDB::init())));
    let compiler = Compiler::new_with_db(interval, Arc::clone(sled_db));
    let compiler = Box::into_raw(Box::new(compiler));
    compiler as *mut compiler_t
}

#[no_mangle]
pub extern "C" fn release_compiler(compiler: *mut compiler_t) {
    if !compiler.is_null() {
        // this will free cache when it goes out of scope
        let _ = unsafe { Box::from_raw(compiler as *mut Compiler) };
    }
}

#[tokio::main]
#[no_mangle]
pub async extern "C" fn start_routine(compiler_ptr: *mut compiler_t) {
    let compiler = match to_compiler(compiler_ptr) {
        Some(compiler) => compiler,
        None => {
            panic!("Failed to get compiler");
        }
    };
    let routine = compiler.routine_fn();
    if let Err(err) = routine.await {
        println!("While compiling, Err: {err:#?}");
    };
}

// byte slice view: golang data type
// unamangedvector: ffi safe vector data type compliants with rust's ownership and data types, for returning optional error value
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct evm_t {}

pub fn to_evm<'a>(ptr: *mut evm_t) -> Option<&'a mut Evm<'a, ExternalContext, GoStorage<'a>>> {
    if ptr.is_null() {
        None
    } else {
        let evm = unsafe { &mut *(ptr as *mut Evm<'a, ExternalContext, GoStorage<'a>>) };
        Some(evm)
    }
}

// initialize vm instance with handler
// if aot mark is true, initialize compiler
#[no_mangle]
pub async extern "C" fn init_vm(default_spec_id: u8, compiler: *mut compiler_t) -> *mut evm_t {
    let db = Db::default();
    let go_storage = GoStorage::new(&db);
    let spec = SpecId::try_from_u8(default_spec_id).unwrap_or(SpecId::CANCUN);
    let builder = EvmBuilder::default();

    let evm = if compiler.is_null() {
        builder.with_db(go_storage).with_spec_id(spec).build()
    } else {
        let ext = ExternalContext::default();
        builder
            .with_db(go_storage)
            .with_spec_id(spec)
            .with_external_context::<ExternalContext>(ext)
            .append_handler_register(register_handler)
            .build()
    };

    let vm = Box::into_raw(Box::new(evm));
    vm as *mut evm_t
}

#[no_mangle]
pub extern "C" fn release_vm(vm: *mut evm_t) {
    if !vm.is_null() {
        // this will free cache when it goes out of scope
        let _ = unsafe { Box::from_raw(vm as *mut Evm<(), GoStorage>) };
    }
}

// VM initializer
#[no_mangle]
pub extern "C" fn execute_tx(
    vm_ptr: *mut evm_t,
    db: Db,
    block: ByteSliceView,
    tx: ByteSliceView,
    errmsg: Option<&mut UnmanagedVector>,
) -> UnmanagedVector {
    let evm = match to_evm(vm_ptr) {
        Some(vm) => vm,
        None => {
            panic!("Failed to get VM");
        }
    };

    let go_storage = GoStorage::new(&db);
    evm.context.evm.db = go_storage;

    set_evm_env(evm, block, tx);

    let result = evm.transact_commit();
    let data = match result {
        Ok(res) => build_flat_buffer(res),
        Err(err) => {
            set_error(err, errmsg);
            Vec::new()
        }
    };

    UnmanagedVector::new(Some(data))
}

#[no_mangle]
pub extern "C" fn query_tx(
    vm_ptr: *mut evm_t,
    db: Db,
    block: ByteSliceView,
    tx: ByteSliceView,
    errmsg: Option<&mut UnmanagedVector>,
) -> UnmanagedVector {
    let evm = match to_evm(vm_ptr) {
        Some(vm) => vm,
        None => {
            panic!("Failed to get VM");
        }
    };
    let go_storage = GoStorage::new(&db);
    evm.context.evm.db = go_storage;

    set_evm_env(evm, block, tx);
    // transact without state commit
    let result = evm.transact();
    let data = match result {
        Ok(res) => {
            println!("Execute_tx: {res:#?}");
            build_flat_buffer(res.result)
        }
        Err(err) => {
            set_error(err, errmsg);
            Vec::new()
        }
    };

    UnmanagedVector::new(Some(data))
}
