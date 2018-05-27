//! Connection contexts for asynchronous communication with a server.
//! 
//! A `Context` object wraps a connection to a PulseAudio server using its native protocol.

// This file is part of the PulseAudio Rust language binding.
//
// Copyright (c) 2017 Lyndon Brown
//
// This library is free software; you can redistribute it and/or modify it under the terms of the
// GNU Lesser General Public License as published by the Free Software Foundation; either version
// 2.1 of the License, or (at your option) any later version.
//
// This library is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without
// even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this library;
// if not, see <http://www.gnu.org/licenses/>.

//! # Overview
//!
//! The asynchronous API is the native interface to the PulseAudio library. It allows full access to
//! all available functionality. This however means that it is rather complex and can take some time
//! to fully master.
//!
//! # Main Loop Abstraction
//!
//! The API is based around an asynchronous event loop, or main loop, abstraction. This abstraction
//! contains three basic elements:
//!
//! * Deferred events: Events that will trigger as soon as possible. Note that some implementations
//!   may block all other events when a deferred event is active.
//! * I/O events: Events that trigger on file descriptor activities.
//! * Timer events: Events that trigger after a fixed amount of time.
//!
//! The abstraction is represented as a number of function pointers in the
//! [`::mainloop::api::MainloopApi`] structure.
//!
//! To actually be able to use these functions, an implementation needs to be coupled to the
//! abstraction. There are three of these shipped with PulseAudio, but any other can be used with a
//! minimal amount of work, provided it supports the three basic events listed above.
//!
//! The implementations shipped with PulseAudio are:
//!
//! * 'Standard': A minimal but fast implementation based on poll().
//! * 'Threaded': A special version of the previous implementation where all of PulseAudio's
//!   internal handling runs in a separate thread.
//! * 'Glib': A wrapper around GLib's main loop. This is provided in the separate
//!   `libpulse_glib_binding` crate.
//!
//! UNIX signals may be hooked to a main loop using the functions from [`::mainloop::signal`]. These
//! rely only on the main loop abstraction and can therefore be used with any of the
//! implementations.
//!
//! # Reference Counting
//!
//! Almost all objects in PulseAudio are reference counted. What that means is that you rarely
//! malloc() or free() any objects. Instead you increase and decrease their reference counts.
//! Whenever an object's reference count reaches zero, that object gets destroyed and any resources
//! it uses get freed.
//!
//! The benefit of this design is that an application need not worry about whether or not it needs
//! to keep an object around in case the library is using it internally. If it is, then it has made
//! sure it has its own reference to it.
//!
//! Whenever the library creates an object, it will have an initial reference count of one. Most of
//! the time, this single reference will be sufficient for the application, so all required
//! reference count interaction will be a single call to the object's `unref` function.
//!
//! Interacting with PulseAudio through this Rust binding, pointers to most reference counted
//! objects are held in a wrapper object, which has an implementation of the `Drop` trait, which is
//! automatically called upon the owned wrapper object going out of scope, and calls the PulseAudio
//! `unref` function. Should use of this binding require increasing the ref count further, there is
//! a choice of either possibly using the raw PulseAudio `ref`/`unref` functions with the underlying
//! C API object pointer, if available, or, preferably, using Rust `Rc`/`Arc` wrappers.
//!
//! # Context
//!
//! A context is the basic object for a connection to a PulseAudio server. It multiplexes commands,
//! data streams and events through a single channel.
//!
//! There is no need for more than one context per application, unless connections to multiple
//! servers are needed.
//!
//! # Operations
//!
//! All operations on the context are performed asynchronously. I.e. the client will not wait for
//! the server to complete the request. To keep track of all these in-flight operations, the
//! application is given an [`::operation::Operation`] object for each asynchronous operation.
//!
//! There are only two actions (besides reference counting) that can be performed on an
//! [`::operation::Operation`]: querying its state with [`::operation::Operation::get_state`] and
//! aborting it with [`::operation::Operation::cancel`].
//!
//! An [`::operation::Operation`] object is reference counted, so an application must make sure to
//! unreference it, even if it has no intention of using it. This however is taken care of
//! automatically in this Rust binding via the implementation of the `Drop` trait on the object.
//!
//! # Connecting
//!
//! A context must be connected to a server before any operation can be issued. Calling
//! [`Context::connect`] will initiate the connection procedure. Unlike most asynchronous
//! operations, connecting does not result in an [`::operation::Operation`] object. Instead, the
//! application should register a callback using [`Context::set_state_callback`].
//!
//! # Disconnecting
//!
//! When the sound support is no longer needed, the connection needs to be closed using
//! [`Context::disconnect`]. This is an immediate function that works synchronously.
//!
//! Since the context object has references to other objects it must be disconnected after use or
//! there is a high risk of memory leaks. If the connection has terminated by itself, then there is
//! no need to explicitly disconnect the context using [`Context::disconnect`].
//!
//! # Functions
//!
//! The sound server's functionality can be divided into a number of subsections:
//!
//! * [`::stream`]
//! * [`::context::scache`]
//! * [`::context::introspect`]
//! * [`::context::subscribe`]
//!
//! [`Context::connect`]: struct.Context.html#method.connect
//! [`Context::disconnect`]: struct.Context.html#method.disconnect
//! [`Context::set_state_callback`]: struct.Context.html#method.set_state_callback
//! [`::context::introspect`]: ../context/introspect/index.html 
//! [`::context::scache`]: ../context/scache/index.html
//! [`::context::subscribe`]: ../context/subscribe/index.html
//! [`::mainloop::api::MainloopApi`]: ../mainloop/api/struct.MainloopApi.html
//! [`::mainloop::signal`]: ../mainloop/signal/index.html
//! [`::operation::Operation::cancel`]: ../operation/struct.Operation.html#method.cancel
//! [`::operation::Operation::get_state`]: ../operation/struct.Operation.html#method.get_state
//! [`::operation::Operation`]: ../operation/struct.Operation.html
//! [`::stream`]: ../stream/index.html

pub mod ext_device_manager;
pub mod ext_device_restore;
pub mod ext_stream_restore;
pub mod introspect;
pub mod scache;
pub mod subscribe;

use std;
use capi;
use std::os::raw::{c_char, c_void};
use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};
use ::mainloop::events::timer::{TimeEvent, TimeEventCb};
use ::util::unwrap_optional_callback;
use ::operation::Operation;

pub use capi::pa_context as ContextInternal;

/// An opaque connection context to a daemon
/// This acts as a safe Rust wrapper for the actual C object.
pub struct Context {
    /// The actual C object.
    pub(crate) ptr: *mut ContextInternal,
    /// Used to avoid freeing the internal object when used as a weak wrapper in callbacks
    weak: bool,
}

/// The state of a connection context
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum State {
    /// The context hasn't been connected yet.
    Unconnected,
    /// A connection is being established.
    Connecting,
    /// The client is authorizing itself to the daemon.
    Authorizing,
    /// The client is passing its application name to the daemon.
    SettingName,
    /// The connection is established, the context is ready to execute operations.
    Ready,
    /// The connection failed or was disconnected.
    Failed,
    /// The connection was terminated cleanly.
    Terminated,
}

impl From<State> for capi::pa_context_state_t {
    fn from(s: State) -> Self {
        unsafe { std::mem::transmute(s) }
    }
}

impl From<capi::pa_context_state_t> for State {
    fn from(s: capi::pa_context_state_t) -> Self {
        unsafe { std::mem::transmute(s) }
    }
}

impl State {
    /// Returns `true` if the passed state is one of the connected states.
    pub fn is_good(self) -> bool {
        self == State::Connecting ||
        self == State::Authorizing ||
        self == State::SettingName ||
        self == State::Ready
    }
}

pub type FlagSet = capi::pa_context_flags_t;

/// Some special flags for contexts.
pub mod flags {
    use capi;
    use super::FlagSet;

    pub const NOFLAGS: FlagSet = capi::PA_CONTEXT_NOFLAGS;
    /// Disable autospawning of the PulseAudio daemon if required.
    pub const NOAUTOSPAWN: FlagSet = capi::PA_CONTEXT_NOAUTOSPAWN;
    /// Don't fail if the daemon is not available when
    /// [`Context::connect`](../struct.Context.html#method.connect) is called, instead enter
    /// [`State::Connecting`](../enum.State.html#Connecting.v) state and wait for the daemon to
    /// appear.
    pub const NOFAIL: FlagSet = capi::PA_CONTEXT_NOFAIL;
}

/// Generic notification callback prototype
pub type ContextNotifyCb = extern "C" fn(c: *mut ContextInternal,
    userdata: *mut c_void);

/// A generic callback for operation completion
/// The `success` param with be zero on success, non-zero otherwise.
pub type ContextSuccessCb = extern "C" fn(c: *mut ContextInternal, success: i32,
    userdata: *mut c_void);

/// A callback for asynchronous meta/policy event messages. The set of defined events can be
/// extended at any time. Also, server modules may introduce additional message types so make sure
/// that your callback function ignores messages it doesn't know.
pub type ContextEventCb = extern "C" fn(c: *mut ContextInternal,
    name: *const c_char, p: *mut ::proplist::ProplistInternal, userdata: *mut c_void);

impl Context {
    /// Instantiate a new connection context with an abstract mainloop API and an application name.
    ///
    /// It is recommended to use [`new_with_proplist`](#method.new_with_proplist) instead and
    /// specify some initial properties.
    pub fn new(mainloop_api: &mut ::mainloop::api::MainloopApi,
        name: &str) -> Option<Self>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_name = CString::new(name.clone()).unwrap();
        let ptr = unsafe { capi::pa_context_new(std::mem::transmute(mainloop_api), c_name.as_ptr()) };
        if ptr.is_null() {
            return None;
        }
        Some(Self::from_raw(ptr))
    }

    /// Instantiate a new connection context with an abstract mainloop API and an application name,
    /// and specify the initial client property list.
    pub fn new_with_proplist(mainloop_api: &mut ::mainloop::api::MainloopApi, name: &str,
        proplist: &::proplist::Proplist) -> Option<Self>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_name = CString::new(name.clone()).unwrap();
        let ptr = unsafe { capi::pa_context_new_with_proplist(
            std::mem::transmute(mainloop_api), c_name.as_ptr(), proplist.ptr) };
        if ptr.is_null() {
            return None;
        }
        Some(Self::from_raw(ptr))
    }

    /// Create a new `Context` from an existing [`ContextInternal`](enum.ContextInternal.html)
    /// pointer.
    pub(crate) fn from_raw(ptr: *mut ContextInternal) -> Self {
        assert_eq!(false, ptr.is_null());
        Self { ptr: ptr, weak: false }
    }

    /// Create a new `Context` from an existing [`ContextInternal`](enum.ContextInternal.html)
    /// pointer. This is the 'weak' version, for use in callbacks, which avoids destroying the
    /// internal object when dropped.
    pub fn from_raw_weak(ptr: *mut ContextInternal) -> Self {
        assert_eq!(false, ptr.is_null());
        Self { ptr: ptr, weak: true }
    }

    /// Set a callback function that is called whenever the context status changes.
    pub fn set_state_callback(&self, cb: Option<(ContextNotifyCb, *mut c_void)>) {
        let (cb_f, cb_d) = unwrap_optional_callback::<ContextNotifyCb>(cb);
        unsafe { capi::pa_context_set_state_callback(self.ptr, cb_f, cb_d); }
    }

    /// Set a callback function that is called whenever a meta/policy control event is received.
    pub fn set_event_callback(&self, cb: Option<(ContextEventCb, *mut c_void)>) {
        let (cb_f, cb_d) = unwrap_optional_callback::<ContextEventCb>(cb);
        unsafe { capi::pa_context_set_event_callback(self.ptr, cb_f, cb_d); }
    }

    /// Returns the error number of the last failed operation
    pub fn errno(&self) -> i32 {
        unsafe { capi::pa_context_errno(self.ptr) }
    }

    /// Returns `true` if some data is pending to be written to the connection
    pub fn is_pending(&self) -> bool {
        unsafe { capi::pa_context_is_pending(self.ptr) != 0 }
    }

    /// Returns the current context status
    pub fn get_state(&self) -> State {
        unsafe { capi::pa_context_get_state(self.ptr).into() }
    }

    /// Connect the context to the specified server.
    ///
    /// If server is `None`, connect to the default server. This routine may but will not always
    /// return synchronously on error. Use [`set_state_callback`](#method.set_state_callback) to be
    /// notified when the connection is established. If `flags` doesn't have
    /// [`flags::NOAUTOSPAWN`](flags/constant.NOAUTOSPAWN.html) set and no specific server is specified
    /// or accessible, a new daemon is spawned. If `api` is not `None`, the functions specified in
    /// the structure are used when forking a new child process.
    pub fn connect(&self, server: Option<&str>, flags: FlagSet, api: Option<&::def::SpawnApi>
        ) -> Result<(), i32>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_server = match server {
            Some(server) => CString::new(server.clone()).unwrap(),
            None => CString::new("").unwrap(),
        };

        let p_api: *const capi::pa_spawn_api = match api {
            Some(api) => unsafe { std::mem::transmute(api) },
            None => null::<capi::pa_spawn_api>(),
        };
        let p_server: *const c_char = match server {
            Some(_) => c_server.as_ptr(),
            None => null::<c_char>(),
        };

        match unsafe { capi::pa_context_connect(self.ptr, p_server, flags, p_api) } {
            0 => Ok(()),
            e => Err(e),
        }
    }

    /// Terminate the context connection immediately.
    pub fn disconnect(&self) {
        unsafe { capi::pa_context_disconnect(self.ptr); }
    }

    /// Drain the context.
    /// If there is nothing to drain, the function returns `None`.
    pub fn drain(&self, cb: (ContextNotifyCb, *mut c_void)) -> Option<Operation> {
        let ptr = unsafe { capi::pa_context_drain(self.ptr, Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Tell the daemon to exit.
    ///
    /// The returned operation is unlikely to complete successfully, since the daemon probably died
    /// before returning a success notification.
    pub fn exit_daemon(&self, cb: (ContextSuccessCb, *mut c_void)) -> Option<Operation> {
        let ptr = unsafe { capi::pa_context_exit_daemon(self.ptr, Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Set the name of the default sink.
    pub fn set_default_sink(&self, name: &str, cb: (ContextSuccessCb, *mut c_void)
        ) -> Option<Operation>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_name = CString::new(name.clone()).unwrap();
        let ptr = unsafe { capi::pa_context_set_default_sink(self.ptr, c_name.as_ptr(), Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Set the name of the default source.
    pub fn set_default_source(&self, name: &str, cb: (ContextSuccessCb, *mut c_void)
        ) -> Option<Operation>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_name = CString::new(name.clone()).unwrap();
        let ptr = unsafe { capi::pa_context_set_default_source(self.ptr, c_name.as_ptr(),
            Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Returns `true` when the connection is to a local daemon. Returns `None` on error, for
    /// instance when no connection has been made yet.
    pub fn is_local(&self) -> Option<bool> {
        match unsafe { capi::pa_context_is_local(self.ptr) } {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }

    /// Set a different application name for context on the server.
    pub fn set_name(&self, name: &str, cb: (ContextSuccessCb, *mut c_void)) -> Option<Operation> {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_name = CString::new(name.clone()).unwrap();
        let ptr = unsafe { capi::pa_context_set_name(self.ptr, c_name.as_ptr(), Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Return the server name this context is connected to.
    pub fn get_server(&self) -> Option<String> {
        let ptr = unsafe { capi::pa_context_get_server(self.ptr) };
        if ptr.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() })
    }

    /// Return the protocol version of the library.
    pub fn get_protocol_version(&self) -> u32 {
        unsafe { capi::pa_context_get_protocol_version(self.ptr) }
    }

    /// Return the protocol version of the connected server.
    ///
    /// Returns `None` on error.
    pub fn get_server_protocol_version(&self) -> Option<u32> {
        match unsafe { capi::pa_context_get_server_protocol_version(self.ptr) } {
            ::def::INVALID_INDEX => None,
            r => Some(r),
        }
    }

    /// Update the property list of the client, adding new entries.
    ///
    /// Please note that it is highly recommended to set as many properties initially via
    /// [`new_with_proplist`](#method.new_with_proplist) as possible instead a posteriori with this
    /// function, since that information may then be used to route streams of the client to the
    /// right device.
    pub fn proplist_update(&self, mode: ::proplist::UpdateMode, p: &mut ::proplist::Proplist,
        cb: (ContextSuccessCb, *mut c_void)) -> Option<Operation>
    {
        let ptr = unsafe { capi::pa_context_proplist_update(self.ptr, mode, p.ptr, Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Update the property list of the client, remove entries.
    pub fn proplist_remove(&self, keys: &[&str], cb: (ContextSuccessCb, *mut c_void)
        ) -> Option<Operation>
    {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let mut c_keys: Vec<CString> = Vec::with_capacity(keys.len());
        for key in keys {
            c_keys.push(CString::new(key.clone()).unwrap());
        }

        // Capture array of pointers to the above CString values.
        // We also add a NULL pointer entry on the end, as expected by the C function called here.
        let mut c_key_ptrs: Vec<*const c_char> = Vec::with_capacity(c_keys.len()+1);
        for c_key in c_keys {
            c_key_ptrs.push(c_key.as_ptr());
        }
        c_key_ptrs.push(null());

        let ptr = unsafe { capi::pa_context_proplist_remove(self.ptr, c_key_ptrs.as_ptr(),
            Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(Operation::from_raw(ptr))
    }

    /// Return the client index this context is identified in the server with.
    ///
    /// This is useful for usage with the introspection functions, such as
    /// [`::introspect::Introspector::get_client_info`].
    ///
    /// Returns `None` on error.
    ///
    /// [`::introspect::Introspector::get_client_info`]: introspect/struct.Introspector.html#method.get_client_info
    pub fn get_index(&self) -> Option<u32> {
        match unsafe { capi::pa_context_get_index(self.ptr) } {
            ::def::INVALID_INDEX => None,
            r => Some(r),
        }
    }

    /// Create a new timer event source for the specified time (wrapper for
    /// [`::mainloop::api::MainloopApi.time_new`]).
    ///
    /// A reference to the mainloop object is needed, in order to associate the event object with
    /// it. The association is done to ensure the even does not outlive the mainloop.
    ///
    /// If pointer returned by underlying C function is `NULL`, `None` will be returned, otherwise a
    /// [`::mainloop::events::timer::TimeEvent`] object will be returned.
    ///
    /// [`::mainloop::events::timer::TimeEvent`]: ../mainloop/events/timer/struct.TimeEvent.html
    /// [`::mainloop::api::MainloopApi.time_new`]: ../mainloop/api/struct.MainloopApi.html#structfield.time_new
    pub fn rttime_new<T>(&self, mainloop: &::mainloop::api::Mainloop<MI=T::MI>,
        usec: ::sample::Usecs, cb: (TimeEventCb, *mut c_void)) -> Option<TimeEvent<T::MI>>
        where T: ::mainloop::api::Mainloop
    {
        let ptr = unsafe { capi::pa_context_rttime_new(self.ptr, usec, Some(cb.0), cb.1) };
        if ptr.is_null() {
            return None;
        }
        Some(TimeEvent::<T::MI>::from_raw(ptr, mainloop.inner().clone()))
    }

    /// Restart a running or expired timer event source (wrapper for
    /// [`::mainloop::api::MainloopApi.time_restart`]).
    ///
    /// [`::mainloop::api::MainloopApi.time_restart`]: ../mainloop/api/struct.MainloopApi.html#structfield.time_restart
    pub fn rttime_restart<T>(&self, e: &TimeEvent<T::MI>, usec: ::sample::Usecs)
        where T: ::mainloop::api::Mainloop
    {
        unsafe { capi::pa_context_rttime_restart(self.ptr, e.get_ptr(), usec); }
    }

    /// Return the optimal block size for passing around audio buffers.
    ///
    /// It is recommended to allocate buffers of the size returned here when writing audio data to
    /// playback streams, if the latency constraints permit this. It is not recommended writing
    /// larger blocks than this because usually they will then be split up internally into chunks of
    /// this size. It is not recommended writing smaller blocks than this (unless required due to
    /// latency demands) because this increases CPU usage.
    ///
    /// If `ss` is invalid, returns `None`, else returns tile size rounded down to multiple of the
    /// frame size. This is supposed to be used in a construct such as:
    ///
    /// ```rust,ignore
    /// let size = stream.get_context().get_tile_size(
    ///     stream.get_sample_spec().unwrap()).unwrap();
    /// ```
    pub fn get_tile_size(&self, ss: &::sample::Spec) -> Option<usize> {
        // Note: C function doc comments mention possibility of passing in a NULL pointer for ss.
        // We do not allow this, since 
        match unsafe { capi::pa_context_get_tile_size(self.ptr, std::mem::transmute(ss)) } {
            std::usize::MAX => None,
            r => Some(r),
        }
    }

    /// Load the authentication cookie from a file.
    ///
    /// This function is primarily meant for PulseAudio's own tunnel modules, which need to load the
    /// cookie from a custom location. Applications don't usually need to care about the cookie at
    /// all, but if it happens that you know what the authentication cookie is and your application
    /// needs to load it from a non-standard location, feel free to use this function.
    pub fn load_cookie_from_file(&self, cookie_file_path: &str) -> Result<(), i32> {
        // Warning: New CStrings will be immediately freed if not bound to a variable, leading to
        // as_ptr() giving dangling pointers!
        let c_path = CString::new(cookie_file_path.clone()).unwrap();
        match unsafe { capi::pa_context_load_cookie_from_file(self.ptr, c_path.as_ptr()) } {
            0 => Ok(()),
            e => Err(e),
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if !self.weak {
            unsafe { capi::pa_context_unref(self.ptr) };
        }
        self.ptr = null_mut::<ContextInternal>();
    }
}

impl Clone for Context {
    /// Returns a new `Context` struct. If this is called on a 'weak' instance, a non-weak object is
    /// returned.
    fn clone(&self) -> Self {
        unsafe { capi::pa_context_ref(self.ptr) };
        Self::from_raw(self.ptr)
    }
}