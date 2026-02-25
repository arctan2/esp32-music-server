#![allow(unused)]

use core::ops::{Deref, DerefMut};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex};
use embassy_sync::signal::Signal as EmbassySignal;
use embassy_sync::channel::Channel as EmbassyChannel;
use embassy_sync::mutex::{Mutex as EmbassyMutex, MutexGuard as EmbassyMutexGuard};
use embassy_sync::channel::Receiver as EmbassyReceiver;
use embassy_sync::channel::Sender as EmbassySender;

#[derive(Debug)]
pub struct Channel<T, const N: usize> {
    ch: EmbassyChannel<CriticalSectionRawMutex, T, N>,
}

// #[derive(Debug)]
// pub struct Sender<T, const N: usize> {
//     s: EmbassySender<'static, CriticalSectionRawMutex, T, N>
// }
// 
// impl <T, const N: usize> Clone for Sender<T, N> {
//     fn clone(&self) -> Self {
//         Self { s: self.s.clone() }
//     }
// }
// 
// impl<T, const N: usize> Sender<T, N> {
//     pub async fn send(&self, msg: T) {
//         self.s.send(msg).await;
//     }
// }
// 
// #[derive(Debug)]
// pub struct Receiver<T, const N: usize> {
//     s: EmbassyReceiver<'static, CriticalSectionRawMutex, T, N>
// }
// 
// impl<T, const N: usize> Receiver<T, N> {
//     pub async fn recv(&self) -> T {
//         self.s.receive().await
//     }
// 
//     pub async fn try_recv(&self) -> Option<T> {
//         self.s.try_receive().ok()
//     }
// 
//     pub async fn is_empty(&self) -> bool {
//         self.s.is_empty()
//     }
// }

impl<T, const N: usize> Channel<T, N> {
    pub const fn new() -> Self {
        Self { ch: EmbassyChannel::new() }
    }

    // pub fn sender(&self) -> Sender<T, N> {
    //     Sender { s: self.ch.sender() }
    // }

    // pub fn receiver(&self) -> Receiver<T, N> {
    //     Receiver { s: self.ch.receiver() }
    // }

    pub async fn recv(&self) -> T {
        self.ch.receive().await
    }

    pub async fn send(&self, msg: T) {
        self.ch.send(msg).await
    }

    pub async fn try_recv(&self) -> Option<T> {
        self.ch.try_receive().ok()
    }

    pub async fn is_empty(&self) -> bool {
        self.ch.is_empty()
    }

    pub async fn is_full(&self) -> bool {
        self.ch.len() == self.ch.capacity()
    }

    pub async fn len(&self) -> usize {
        self.ch.len()
    }
}

pub struct Signal<T> {
    sig: EmbassySignal<CriticalSectionRawMutex, T>,
}

impl<T> Signal<T> {
    pub const fn new() -> Self {
        Self { sig: EmbassySignal::new() }
    }

    pub async fn wait(&self) -> T {
        self.sig.wait().await
    }

    pub async fn signal(&self, v: T) {
        self.sig.signal(v)
    }

    pub async fn reset(&self) {
        if self.sig.signaled() {
            self.sig.reset();
        }
    }
}

#[derive(Debug)]
pub struct Mutex<T> {
    m: EmbassyMutex<CriticalSectionRawMutex, T>
}

#[derive(Debug)]
pub struct MutexGuard<'a, T> {
    g: EmbassyMutexGuard<'a, CriticalSectionRawMutex, T>
}

impl <T> Mutex<T> {
    pub fn new(val: T) -> Self {
        Self { m: EmbassyMutex::new(val) }
    }

    pub async fn lock<'a>(&'a self) -> MutexGuard<'a, T> {
        MutexGuard { g: self.m.lock().await }
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.g
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.g
    }
}
