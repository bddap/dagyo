use std::collections::HashMap;
use std::env;
use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;

use anyhow::{anyhow as ah, Context};
use dagyo::flow::{JobDesc, MailBox};
use dashmap::DashMap;
use futures::StreamExt;
use lapin::options::{BasicGetOptions, QueueDeclareOptions};
use lapin::{
    message::Delivery,
    options::{BasicConsumeOptions, BasicQosOptions},
    types::FieldTable,
};
use proto::{
    dagyo_sidecar_server::DagyoSidecarServer, Input, PanicRequest, PutOutputRequest, StreamId,
};
use tokio::sync::Mutex as Tutex;
use tonic::{Request, Response, Status, Streaming};

pub mod proto {
    tonic::include_proto!("dagyo_sidecar");
}

struct InputStream {
    consumer: Arc<lapin::Channel>,
    queue: lapin::Queue,
    done: AtomicBool,
}

impl InputStream {
    async fn from_mailbox(channel: &lapin::Channel, mailbox: MailBox) -> anyhow::Result<Self> {
        unimplemented!()
    }

    async fn next(&self) -> anyhow::Result<Option<Vec<u8>>> {
        if self.done.load(atomic::Ordering::SeqCst) {
            return Ok(None);
        }

        let delivery: Delivery = self
            .consumer
            .basic_get(self.queue.name().as_str(), BasicGetOptions { no_ack: true })
            .await?
            .ok_or_else(|| ah!("Input stream ended without explicit close message."))?
            .delivery;

        // The single byte message [0x01] is the the explicit end-of-stream marker.
        // [0x00, ..body] is a valid message.
        // Any other message is invalid.
        match delivery.data.as_slice() {
            [0x01] => {
                self.done.store(true, atomic::Ordering::SeqCst);
                Ok(None)
            }
            [0x00, ..] => {
                let mut body = delivery.data;
                body.remove(0);
                Ok(Some(body))
            }
            [] => Err(ah!("Input stream got an invalid message; no prefix.")),
            _ => Err(ah!(
                "Input stream got an invalid message; unrecognized prefix."
            )),
        }
    }
}

struct OutputStream {
    channel: Arc<lapin::Channel>,
    queue: lapin::Queue,
    done: AtomicBool,
}

impl OutputStream {
    async fn from_mailbox(channel: Arc<lapin::Channel>, mailbox: MailBox) -> anyhow::Result<Self> {
        let queue_name = mailbox.queue_name();

        // assert the queue exists using passive declare
        let queue = channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        Ok(Self {
            channel,
            queue,
            done: AtomicBool::new(false),
        })
    }

    async fn close(&self) -> anyhow::Result<()> {
        if self.done.load(atomic::Ordering::SeqCst) {
            return Ok(());
        }

        self.channel
            .basic_publish(
                "",
                self.queue.name().as_str(),
                Default::default(),
                &[0x01],
                Default::default(),
            )
            .await?;

        self.done.store(true, atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, message: Vec<u8>) -> anyhow::Result<()> {
        if self.done.load(atomic::Ordering::SeqCst) {
            return Err(ah!("Output stream is closed."));
        }

        let mut payload = message;
        payload.insert(0, 0x00);

        self.channel
            .basic_publish(
                "",
                self.queue.name().as_str(),
                Default::default(),
                &payload,
                Default::default(),
            )
            .await?;

        Ok(())
    }
}

struct Job {
    inputs: HashMap<String, InputStream>,
    outputs: HashMap<String, OutputStream>,
    panic: OutputStream,
    health: OutputStream,
    stop: InputStream,
}

impl Job {
    async fn connect(channel: Arc<lapin::Channel>, desc: JobDesc) -> anyhow::Result<Self> {
        let mut inputs = HashMap::new();
        for (k, v) in desc.inputs {
            let stream = InputStream::from_mailbox(&channel, v).await?;
            inputs.insert(k, stream);
        }

        let mut outputs = HashMap::new();
        for (k, v) in desc.outputs {
            let stream = OutputStream::from_mailbox(Arc::clone(&channel), v).await?;
            outputs.insert(k, stream);
        }

        let stop = InputStream::from_mailbox(&channel, desc.stop).await?;
        let panic = OutputStream::from_mailbox(Arc::clone(&channel), desc.panic).await?;
        let health = OutputStream::from_mailbox(channel, desc.health).await?;

        Ok(Self {
            inputs,
            outputs,
            panic,
            health,
            stop,
        })
    }
}

pub struct Sidecar {
    message_broker: Arc<lapin::Channel>,
    job_queue: Tutex<lapin::Consumer>,
    jobs: DashMap<u64, Job>,
}

impl Sidecar {
    pub async fn init() -> anyhow::Result<Self> {
        assert_eq!(
            BasicConsumeOptions::default(),
            BasicConsumeOptions {
                no_local: false,
                no_ack: false,
                exclusive: false,
                nowait: false,
            },
        );

        let job_queue_name = env::var("DAGYO_QUEUE")?;
        let amqp_url = env::var("AMQP_URL")?;
        let connection = lapin::Connection::connect(&amqp_url, Default::default()).await?;
        let channel = connection.create_channel().await?;
        channel.basic_qos(1, BasicQosOptions::default()).await?;
        let job_queue = channel
            .basic_consume(
                &job_queue_name,
                "dagyo_sidecar",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(Self {
            message_broker: Arc::new(channel),
            job_queue: Tutex::new(job_queue),
            jobs: DashMap::new(),
        })
    }

    pub async fn close_job(&self, job_id: u64) -> anyhow::Result<()> {
        let job = self
            .jobs
            .remove(&job_id)
            .ok_or_else(|| anyhow::anyhow!("Tried to close a non-existent job."))?
            .1;

        // If any stream remains unterminated, thats an error. Panic the job.
        let mut unterminated_inputs = Vec::new();
        for (name, stream) in job.inputs {
            if stream.done.load(atomic::Ordering::SeqCst) {
                unterminated_inputs.push(name);
            }
        }

        let mut unterminated_outputs = Vec::new();
        for (name, stream) in job.outputs {
            if !stream.done.load(atomic::Ordering::SeqCst) {
                unterminated_outputs.push(name);
            }
        }

        if !unterminated_inputs.is_empty() || !unterminated_outputs.is_empty() {
            let message = format!(
                "Job was closed despite unterminated streams.\n  Intput: {:?}\n  Output: {:?}\n",
                unterminated_inputs, unterminated_outputs,
            );
            job.panic.send(message.into()).await?;
        }

        Ok(())
    }

    async fn receive_job_description(&self) -> anyhow::Result<JobDesc> {
        let job: Option<lapin::Result<_>> = self.job_queue.lock().await.next().await;
        let job: lapin::Result<_> = job.ok_or_else(|| anyhow::anyhow!("Queue was empty."))?;
        let job: Delivery = job.map_err(Into::<anyhow::Error>::into)?;
        let job = serde_json::from_slice(&job.data)?;
        Ok(job)
    }
}

#[tonic::async_trait]
impl proto::dagyo_sidecar_server::DagyoSidecar for Arc<Sidecar> {
    async fn take_job(
        &self,
        request: Request<Streaming<()>>,
    ) -> Result<Response<proto::Job>, Status> {
        let job_description = self
            .receive_job_description()
            .await
            .context("popping a job from the queue")
            .map_err(report)?;

        let job = Job::connect(Arc::clone(&self.message_broker), job_description)
            .await
            .context("linking job inputs and outputs to message broker streams")
            .map_err(report)?;

        let job_id = rand::random::<u64>();
        let existing = self.jobs.insert(job_id, job);
        assert!(
            existing.is_none(),
            "Generated key already exists. Consider buying a lottery ticket.",
        );

        // When the request stream is dropped, the job is done. Time to clean up.
        let self_ref = Arc::clone(self);
        tokio::spawn(async move {
            let mut stream = request.into_inner();
            loop {
                match stream.next().await {
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::error!("Failed to read from request stream: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            self_ref.close_job(job_id).await.unwrap_or_else(|e| {
                tracing::error!("Failed to close job: {}", e);
            });
        });

        Ok(Response::new(proto::Job { id: job_id }))
    }

    async fn put_output(&self, request: Request<PutOutputRequest>) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn close_output(&self, request: Request<StreamId>) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn panic(&self, request: Request<PanicRequest>) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn report_healthy_until(
        &self,
        request: Request<proto::ReportHealthyUntilRequest>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn wait_for_stop(&self, request: Request<proto::Job>) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn pop_input(&self, request: Request<StreamId>) -> Result<Response<Input>, Status> {
        let request = request.into_inner();

        let job = self
            .jobs
            .get(&request.job)
            .ok_or_else(|| report(ah!("Tried to get input stream for non-existent job.")))?;

        let input_stream = job
            .inputs
            .get(&request.name)
            .ok_or_else(|| report(ah!("Tried to get non-existent input stream.")))?;

        let next: Option<Vec<u8>> = input_stream.next().await.map_err(report)?;
        // let mess = next.map(|some| Mess { some });
        // Ok(Response::new(Input { mess }))
        umimplemented!()
    }
}

fn report(err: anyhow::Error) -> Status {
    tracing::error!("Error: {}", err);
    Status::new(tonic::Code::Internal, err.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let host = "[::1]:50051".parse()?;
    let sidecar = Sidecar::init().await?;
    tonic::transport::Server::builder()
        .add_service(DagyoSidecarServer::new(Arc::new(sidecar)))
        .serve(host)
        .await?;
    Ok(())
}

// import typing as T
// from types import TracebackType
// from dataclasses import dataclass
// import os
// from aio_pika import connect, Message
// from aio_pika.abc import AbstractQueue, AbstractChannel, AbstractExchange
// from functools import lru_cache
// from pydantic import BaseModel
// from dataclasses import dataclass
// import sys
// import asyncio
// from typing import Callable, Awaitable
// from asyncio import Semaphore

// def eprint(*args, **kwargs) -> None:
//     print(*args, file=sys.stderr, **kwargs)

// @lru_cache()
// async def connect_mq():
//     que = os.environ["DAGYO_QUEUE"]  # e.g. "amqp://guest:guest@localhost/"
//     return await connect(que)

// InStreamAddr: T.TypeAlias = str
// OutStreamAddr: T.TypeAlias = str

// @dataclass
// class InStream:
//     q: AbstractQueue

//     @staticmethod
//     async def create(channel: AbstractChannel, addr: InStreamAddr) -> "InStream":
//         q = await channel.declare_queue(addr, passive=True)
//         return InStream(q)

//     async def __aiter__(self) -> T.AsyncIterator[bytes]:
//         # A message starting with a single byte 0x01 indicates end of stream
//         # an eos message must be exactly 1 byte long.
//         # Any other message is data. The first byte is ignored.
//         async for message in self.q.iterator():
//             await message.ack()
//             assert (
//                 len(message.body) >= 1
//             ), "received an invalid message on and input stream, messages must be at least 1 byte long"
//             if message.body[0] == 1:
//                 assert (
//                     len(message.body) == 1
//                 ), "received an invalid end-of-stream message, eos messages must be exactly 1 byte long"
//                 break
//             yield message.body[1:]

// @dataclass
// class OutStream:
//     exchange: AbstractExchange
//     addr: OutStreamAddr

//     async def send(self, msg: bytes):
//         await self.exchange.publish(Message(b"\0" + msg), routing_key=self.addr)

//     async def close(self):
//         await self.exchange.publish(Message(b"\1"), routing_key=self.addr)

// class JobDesc(BaseModel):
//     """Serialized Job"""

//     inputs: dict[str, InStreamAddr]
//     outputs: dict[str, OutStreamAddr]
//     panic: OutStreamAddr
//     health: OutStreamAddr
//     stop: InStreamAddr

// @dataclass
// class Job:
//     inputs: dict[str, InStream]
//     outputs: dict[str, OutStream]
//     panic: OutStream
//     health: OutStream
//     stop: InStream

//     # report failure
//     async def do_panic(self, note: str):
//         await self.panic.send(note.encode())

//     # send keepalive
//     async def report_healthy(self):
//         await self.health.send(b"")

//     @staticmethod
//     async def create(message_body: bytes, channel: AbstractChannel) -> "Job":
//         jd = JobDesc.parse_raw(message_body)
//         exchange = channel.default_exchange
//         return Job(
//             inputs={k: await InStream.create(channel, v) for k, v in jd.inputs.items()},
//             outputs={k: OutStream(exchange, v) for k, v in jd.outputs.items()},
//             panic=OutStream(exchange, jd.panic),
//             health=OutStream(exchange, jd.health),
//             stop=await InStream.create(channel, jd.stop),
//         )

//     async def __aenter__(self) -> "Job":
//         return self

//     async def __aexit__(
//         self,
//         exc_type: T.Optional[T.Type[BaseException]],
//         exc_value: T.Optional[BaseException],
//         traceback: T.Optional[TracebackType],
//     ) -> bool:
//         _ = exc_type, traceback

//         if exc_value is not None:
//             e = repr(exc_value)
//             eprint("Paniking:", e)
//             await self.do_panic(e)

//         for to_close in [*self.outputs.values(), self.panic, self.health]:
//             await to_close.close()

//         # Is there any way we can assert the stop signal was heeded?
//         # Maybe stop should result in an exception?
//         # Can we panic with the full traceback? Maybe panics should be json.

//         # returning True means we handled the exception
//         return True

// async def jobs() -> T.AsyncIterator[Job]:
//     job_mailbox = os.environ["DAGYO_JOBS"]
//     connection = await connect_mq()

//     async with connection:
//         jobs_channel = await connection.channel()
//         await jobs_channel.set_qos(prefetch_count=1)
//         jobs_queue = await jobs_channel.get_queue(job_mailbox)

//         async for message in jobs_queue.iterator():
//             # tell the queue never to give this job to another executor, even if we fail while running it
//             await message.ack()

//             streams_channel = await connection.channel()
//             yield await Job.create(message.body, streams_channel)

// def blastoff(cb: Callable[[Job], Awaitable[None]], worker_slots: int) -> None:
//     """
//     Run an async callback for each job, with a limit on the number of jobs running
//     at once.
//     Job outputs are automatically closed when the callback returns or raises.
//     If the callback throws an error, a panic output is automatically sent.
//     The callback is run within an asyncio executor.
//     """

//     work_permit = Semaphore(worker_slots)

//     async def run_lock(job: Job) -> None:
//         try:
//             async with job:
//                 await cb(job)
//         finally:
//             work_permit.release()

//     async def main() -> None:
//         jbs = jobs()

//         while True:
//             await work_permit.acquire()
//             try:
//                 job = await anext(jbs)
//             except StopAsyncIteration:
//                 break
//             asyncio.create_task(run_lock(job))

//     asyncio.run(main())
