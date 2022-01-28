// Copyright 2018-2022 the Deno authors. All rights reserved. MIT license.
// @generated file from build script, do not edit
// deno-lint-ignore-file

let cachedTextDecoder = new TextDecoder("utf-8", {
  ignoreBOM: true,
  fatal: true,
});

cachedTextDecoder.decode();

let cachegetUint8Memory0 = null;
function getUint8Memory0() {
  if (
    cachegetUint8Memory0 === null ||
    cachegetUint8Memory0.buffer !== wasm.memory.buffer
  ) {
    cachegetUint8Memory0 = new Uint8Array(wasm.memory.buffer);
  }
  return cachegetUint8Memory0;
}

function getStringFromWasm0(ptr, len) {
  return cachedTextDecoder.decode(getUint8Memory0().subarray(ptr, ptr + len));
}

const heap = new Array(32).fill(undefined);

heap.push(undefined, null, true, false);

let heap_next = heap.length;

function addHeapObject(obj) {
  if (heap_next === heap.length) heap.push(heap.length + 1);
  const idx = heap_next;
  heap_next = heap[idx];

  heap[idx] = obj;
  return idx;
}

function getObject(idx) {
  return heap[idx];
}

let WASM_VECTOR_LEN = 0;

let cachedTextEncoder = new TextEncoder("utf-8");

const encodeString = function (arg, view) {
  return cachedTextEncoder.encodeInto(arg, view);
};

function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === undefined) {
    const buf = cachedTextEncoder.encode(arg);
    const ptr = malloc(buf.length);
    getUint8Memory0().subarray(ptr, ptr + buf.length).set(buf);
    WASM_VECTOR_LEN = buf.length;
    return ptr;
  }

  let len = arg.length;
  let ptr = malloc(len);

  const mem = getUint8Memory0();

  let offset = 0;

  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset);
    if (code > 0x7F) break;
    mem[ptr + offset] = code;
  }

  if (offset !== len) {
    if (offset !== 0) {
      arg = arg.slice(offset);
    }
    ptr = realloc(ptr, len, len = offset + arg.length * 3);
    const view = getUint8Memory0().subarray(ptr + offset, ptr + len);
    const ret = encodeString(arg, view);

    offset += ret.written;
  }

  WASM_VECTOR_LEN = offset;
  return ptr;
}

function isLikeNone(x) {
  return x === undefined || x === null;
}

let cachegetInt32Memory0 = null;
function getInt32Memory0() {
  if (
    cachegetInt32Memory0 === null ||
    cachegetInt32Memory0.buffer !== wasm.memory.buffer
  ) {
    cachegetInt32Memory0 = new Int32Array(wasm.memory.buffer);
  }
  return cachegetInt32Memory0;
}

function dropObject(idx) {
  if (idx < 36) return;
  heap[idx] = heap_next;
  heap_next = idx;
}

function takeObject(idx) {
  const ret = getObject(idx);
  dropObject(idx);
  return ret;
}

const CLOSURE_DTORS = new FinalizationRegistry((state) => {
  wasm.__wbindgen_export_2.get(state.dtor)(state.a, state.b);
});

function makeMutClosure(arg0, arg1, dtor, f) {
  const state = { a: arg0, b: arg1, cnt: 1, dtor };
  const real = (...args) => {
    // First up with a closure we increment the internal reference
    // count. This ensures that the Rust closure environment won't
    // be deallocated while we're invoking it.
    state.cnt++;
    const a = state.a;
    state.a = 0;
    try {
      return f(a, state.b, ...args);
    } finally {
      if (--state.cnt === 0) {
        wasm.__wbindgen_export_2.get(state.dtor)(a, state.b);
        CLOSURE_DTORS.unregister(state);
      } else {
        state.a = a;
      }
    }
  };
  real.original = state;
  CLOSURE_DTORS.register(real, state, state);
  return real;
}
function __wbg_adapter_16(arg0, arg1, arg2) {
  wasm
    ._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h59cc8226d744e022(
      arg0,
      arg1,
      addHeapObject(arg2),
    );
}

function passArray8ToWasm0(arg, malloc) {
  const ptr = malloc(arg.length * 1);
  getUint8Memory0().set(arg, ptr / 1);
  WASM_VECTOR_LEN = arg.length;
  return ptr;
}

function handleError(f, args) {
  try {
    return f.apply(this, args);
  } catch (e) {
    wasm.__wbindgen_exn_store(addHeapObject(e));
  }
}
function __wbg_adapter_43(arg0, arg1, arg2, arg3) {
  wasm.wasm_bindgen__convert__closures__invoke2_mut__h0f5dc0d056795ca5(
    arg0,
    arg1,
    addHeapObject(arg2),
    addHeapObject(arg3),
  );
}

const ParserFinalization = new FinalizationRegistry((ptr) =>
  wasm.__wbg_parser_free(ptr)
);
/** */
export class Parser {
  static __wrap(ptr) {
    const obj = Object.create(Parser.prototype);
    obj.ptr = ptr;
    ParserFinalization.register(obj, obj.ptr, obj);
    return obj;
  }

  __destroy_into_raw() {
    const ptr = this.ptr;
    this.ptr = 0;
    ParserFinalization.unregister(this);
    return ptr;
  }

  free() {
    const ptr = this.__destroy_into_raw();
    wasm.__wbg_parser_free(ptr);
  }
  /** */
  constructor() {
    var ret = wasm.parser_new();
    return Parser.__wrap(ret);
  }
  /**
   * Parse from a BYOB readable stream.
   * @param {ReadableStreamBYOBReader} stream
   * @returns {Promise<any>}
   */
  parse(stream) {
    var ret = wasm.parser_parse(this.ptr, addHeapObject(stream));
    return takeObject(ret);
  }
  /**
   * Parse from an in-memory buffer.
   * @param {Uint8Array} buffer
   * @returns {Promise<any>}
   */
  parseBuffer(buffer) {
    var ptr0 = passArray8ToWasm0(buffer, wasm.__wbindgen_malloc);
    var len0 = WASM_VECTOR_LEN;
    var ret = wasm.parser_parseBuffer(this.ptr, ptr0, len0);
    return takeObject(ret);
  }
  /**
   * Load module sources.
   * @returns {Promise<any>}
   */
  load() {
    var ret = wasm.parser_load(this.ptr);
    return takeObject(ret);
  }
  /**
   * Get a module source.
   * @param {string} specifier
   * @returns {Promise<any>}
   */
  getModuleSource(specifier) {
    var ptr0 = passStringToWasm0(
      specifier,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    );
    var len0 = WASM_VECTOR_LEN;
    var ret = wasm.parser_getModuleSource(this.ptr, ptr0, len0);
    return takeObject(ret);
  }
  /**
   * Get a module sourcemap.
   * @param {string} specifier
   * @returns {Promise<any>}
   */
  getModuleSourceMap(specifier) {
    var ptr0 = passStringToWasm0(
      specifier,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    );
    var len0 = WASM_VECTOR_LEN;
    var ret = wasm.parser_getModuleSourceMap(this.ptr, ptr0, len0);
    return takeObject(ret);
  }
}

const imports = {
  __wbindgen_placeholder__: {
    __wbindgen_string_new: function (arg0, arg1) {
      var ret = getStringFromWasm0(arg0, arg1);
      return addHeapObject(ret);
    },
    __wbindgen_string_get: function (arg0, arg1) {
      const obj = getObject(arg1);
      var ret = typeof (obj) === "string" ? obj : undefined;
      var ptr0 = isLikeNone(ret)
        ? 0
        : passStringToWasm0(
          ret,
          wasm.__wbindgen_malloc,
          wasm.__wbindgen_realloc,
        );
      var len0 = WASM_VECTOR_LEN;
      getInt32Memory0()[arg0 / 4 + 1] = len0;
      getInt32Memory0()[arg0 / 4 + 0] = ptr0;
    },
    __wbg_done_9ec69af688679dee: function (arg0) {
      var ret = getObject(arg0).done;
      return ret;
    },
    __wbg_value_a1cff389ca46d427: function (arg0) {
      var ret = getObject(arg0).value;
      return isLikeNone(ret) ? 0 : addHeapObject(ret);
    },
    __wbindgen_object_drop_ref: function (arg0) {
      takeObject(arg0);
    },
    __wbg_error_09919627ac0992f5: function (arg0, arg1) {
      try {
        console.error(getStringFromWasm0(arg0, arg1));
      } finally {
        wasm.__wbindgen_free(arg0, arg1);
      }
    },
    __wbg_new_693216e109162396: function () {
      var ret = new Error();
      return addHeapObject(ret);
    },
    __wbg_stack_0ddaca5d1abfb52f: function (arg0, arg1) {
      var ret = getObject(arg1).stack;
      var ptr0 = passStringToWasm0(
        ret,
        wasm.__wbindgen_malloc,
        wasm.__wbindgen_realloc,
      );
      var len0 = WASM_VECTOR_LEN;
      getInt32Memory0()[arg0 / 4 + 1] = len0;
      getInt32Memory0()[arg0 / 4 + 0] = ptr0;
    },
    __wbg_read_dd73d50a65d20ec5: function (arg0, arg1) {
      var ret = getObject(arg0).read(getObject(arg1));
      return addHeapObject(ret);
    },
    __wbindgen_cb_drop: function (arg0) {
      const obj = takeObject(arg0).original;
      if (obj.cnt-- == 1) {
        obj.a = 0;
        return true;
      }
      var ret = false;
      return ret;
    },
    __wbg_new_16f24b0728c5e67b: function () {
      var ret = new Array();
      return addHeapObject(ret);
    },
    __wbg_push_a72df856079e6930: function (arg0, arg1) {
      var ret = getObject(arg0).push(getObject(arg1));
      return ret;
    },
    __wbg_call_94697a95cb7e239c: function () {
      return handleError(function (arg0, arg1, arg2) {
        var ret = getObject(arg0).call(getObject(arg1), getObject(arg2));
        return addHeapObject(ret);
      }, arguments);
    },
    __wbg_toString_9b85345d84562096: function (arg0) {
      var ret = getObject(arg0).toString();
      return addHeapObject(ret);
    },
    __wbg_new_4beacc9c71572250: function (arg0, arg1) {
      try {
        var state0 = { a: arg0, b: arg1 };
        var cb0 = (arg0, arg1) => {
          const a = state0.a;
          state0.a = 0;
          try {
            return __wbg_adapter_43(a, state0.b, arg0, arg1);
          } finally {
            state0.a = a;
          }
        };
        var ret = new Promise(cb0);
        return addHeapObject(ret);
      } finally {
        state0.a = state0.b = 0;
      }
    },
    __wbg_resolve_4f8f547f26b30b27: function (arg0) {
      var ret = Promise.resolve(getObject(arg0));
      return addHeapObject(ret);
    },
    __wbg_then_a6860c82b90816ca: function (arg0, arg1) {
      var ret = getObject(arg0).then(getObject(arg1));
      return addHeapObject(ret);
    },
    __wbg_then_58a04e42527f52c6: function (arg0, arg1, arg2) {
      var ret = getObject(arg0).then(getObject(arg1), getObject(arg2));
      return addHeapObject(ret);
    },
    __wbg_new_e3b800e570795b3c: function (arg0) {
      var ret = new Uint8Array(getObject(arg0));
      return addHeapObject(ret);
    },
    __wbg_newwithlength_5f4ce114a24dfe1e: function (arg0) {
      var ret = new Uint8Array(arg0 >>> 0);
      return addHeapObject(ret);
    },
    __wbg_length_30803400a8f15c59: function (arg0) {
      var ret = getObject(arg0).length;
      return ret;
    },
    __wbg_byteLength_8e4d38eb0b20db39: function (arg0) {
      var ret = getObject(arg0).byteLength;
      return ret;
    },
    __wbg_set_5b8081e9d002f0df: function (arg0, arg1, arg2) {
      getObject(arg0).set(getObject(arg1), arg2 >>> 0);
    },
    __wbindgen_is_object: function (arg0) {
      const val = getObject(arg0);
      var ret = typeof (val) === "object" && val !== null;
      return ret;
    },
    __wbg_buffer_5e74a88a1424a2e0: function (arg0) {
      var ret = getObject(arg0).buffer;
      return addHeapObject(ret);
    },
    __wbindgen_throw: function (arg0, arg1) {
      throw new Error(getStringFromWasm0(arg0, arg1));
    },
    __wbindgen_memory: function () {
      var ret = wasm.memory;
      return addHeapObject(ret);
    },
    __wbindgen_closure_wrapper2520: function (arg0, arg1, arg2) {
      var ret = makeMutClosure(arg0, arg1, 91, __wbg_adapter_16);
      return addHeapObject(ret);
    },
  },
};

const wasm_url = new URL("eszip_wasm_bg.wasm", import.meta.url);
let wasmInstantiatePromise;
switch (wasm_url.protocol) {
  case "file:": {
    if ("permissions" in Deno) {
      Deno.permissions.request({ name: "read", path: wasm_url });
    }
    const wasmCode = await Deno.readFile(wasm_url);
    wasmInstantiatePromise = WebAssembly.instantiate(wasmCode, imports);
    break;
  }
  case "https:":
  case "http:": {
    if ("permissions" in Deno) {
      Deno.permissions.request({ name: "net", host: wasm_url.host });
    }
    const wasmResponse = await fetch(wasm_url);
    if (
      wasmResponse.headers.get("content-type")?.toLowerCase().startsWith(
        "application/wasm",
      )
    ) {
      wasmInstantiatePromise = WebAssembly.instantiateStreaming(
        wasmResponse,
        imports,
      );
    } else {
      wasmInstantiatePromise = WebAssembly.instantiate(
        await wasmResponse.arrayBuffer(),
        imports,
      );
    }
    break;
  }
  default:
    throw new Error(`Unsupported protocol: ${wasm_url.protocol}`);
}
const wasmInstance = (await wasmInstantiatePromise).instance;
const wasm = wasmInstance.exports;

/* for testing and debugging */
export const _wasm = wasm;
export const _wasmInstance = wasmInstance;
