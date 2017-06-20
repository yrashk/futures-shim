// Copyright 2017 Yurii Rashkovskii
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! The purpose of this module is to enable smooth
//! integration of unbounded mpsc-using components into
//! futures-based pipelines.

use futures::{Future, Stream, Poll, Async, task};
use std::sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender,
                      TryRecvError, SendError as MpscSendError,
                      channel as mpsc_channel};

/// Wrapper for an mpsc Receiver
///
/// One can create one using the `From` or `Into` trait:
///
/// ```
/// use futures_shim::mpsc;
/// let (_, rx) = ::std::sync::mpsc::channel::<i8>();
/// let _ : mpsc::Receiver<i8> = rx.into();
/// ```
///
/// Receiver also implements futures' `Stream` trait
#[derive(Debug)]
pub struct Receiver<T>(MpscReceiver<T>);

impl<T> From<MpscReceiver<T>> for Receiver<T> {
    fn from(value: MpscReceiver<T>) -> Self {
        Receiver(value)
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.0.try_recv() {
            Ok(result) => Ok(Async::from(Some(result))),
            Err(TryRecvError::Empty) => {
                task::current().notify();
                Ok(Async::NotReady)
            },
            Err(TryRecvError::Disconnected) => Ok(Async::from(None)),
        }
    }
}

/// Wrapper for an mpsc Sender
///
/// One can create one using the `From` or `Into` trait:
///
/// ```
/// use futures_shim::mpsc;
/// let (tx, _) = ::std::sync::mpsc::channel::<i8>();
/// let _ : mpsc::Sender<i8> = tx.into();
/// ```
#[derive(Debug)]
pub struct Sender<T>(MpscSender<T>);

impl<T> From<MpscSender<T>> for Sender<T> {
    fn from(value: MpscSender<T>) -> Self {
        Sender(value)
    }
}

/// Represents an error sending data
///
/// To quote stdlib's documentation:
///
/// A **send** operation can only fail if the receiving end of a channel is
/// disconnected, implying that the data could never be received. The error
/// contains the data being sent as a payload so it can be recovered.
#[derive(Debug)]
pub struct SendError<T>(T);

pub struct SendResult<T>(Option<Result<(), MpscSendError<T>>>);

impl<T> Future for SendResult<T> {
    type Item = ();
    type Error = SendError<T>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = self.0.take().expect("can't poll SendResult twice");
        match result {
            Ok(()) => Ok(Async::from(())),
            Err(MpscSendError(data)) => Err(SendError(data)),
        }
    }
}

impl<T> Sender<T> {
    /// Sends the data, returning a future
    pub fn send(&self, data: T) -> SendResult<T> {
        SendResult(Some(self.0.send(data)))
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.0.clone().into()
    }
}


/// Creates an unbounded mpsc channel
///
/// This function is Using `std::sync::mpsc::channel` behind the scenes, and
/// converting into future_shim's `Sender` and `Receiver`
///
/// ```
/// use futures_shim::mpsc;
/// let (tx, rx) = mpsc::channel::<i8>();
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc_channel();
    (tx.into(), rx.into())
}

#[cfg(test)]
mod tests {

    use mpsc;
    use tokio_core;
    use futures::{Stream, Future};
    use std::thread;

    #[test]
    fn it_works() {
        let mut core = tokio_core::reactor::Core::new().unwrap();
        let (tx, rx) = mpsc::channel();
        assert!(core.run(tx.send(1i8)).is_ok());
        let (received, _) = core.run(rx.into_future()).unwrap();
        assert_eq!(Some(1i8), received);
    }

    #[test]
    fn from_into() {
        let (tx, rx) = ::std::sync::mpsc::channel::<i8>();
        let _ : mpsc::Receiver<i8> = rx.into();
        let _ : mpsc::Sender<i8> = tx.into();
    }

    #[test]
    fn stream_end_on_drop() {
        let mut core = tokio_core::reactor::Core::new().unwrap();
        let (tx, rx) = mpsc::channel::<i8>();
        drop(tx);
        let (received, _) = core.run(rx.into_future()).unwrap();
        assert_eq!(received, None);
    }


    #[test]
    fn does_not_block_on_empty() {
        let mut core = tokio_core::reactor::Core::new().unwrap();
        let (tx, rx) = mpsc::channel();
        let tx1 = tx.clone();
        let remote = core.remote();
        thread::spawn(move || {
            thread::sleep(::std::time::Duration::from_millis(500));
            remote.spawn(move |_| tx1.send(1i8).map_err(|_| ()));
        });
        let (received, _) = core.run(rx.into_future()).unwrap();
        assert_eq!(Some(1i8), received);
    }

}
