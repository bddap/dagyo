use std::env;

use dagyo::flow::JobDesc;
use dashmap::DashMap;
use futures::{Stream, StreamExt};
use lapin::{
    message::Delivery,
    options::{BasicConsumeOptions, BasicQosOptions},
    types::FieldTable,
};
use prost_types::Timestamp;
use proto::{
    dagyo_sidecar_server::DagyoSidecarServer, InputStreamElement, PanicRequest, PutOutputRequest,
    StreamId,
};
use tonic::{Request, Response, Status, Streaming};

pub mod proto {
    tonic::include_proto!("dagyo_sidecar");
}

struct InputStream {
    queue_name: String,
    channel: lapin::Channel,
}

struct OutputStream {
    queue_name: String,
    channel: lapin::Channel,
}

struct Job {
    inputs: DashMap<String, InputStream>,
    outputs: DashMap<String, OutputStream>,
    panic: OutputStream,
    health: OutputStream,
    stop: InputStream,
}

pub struct Sidecar {
    message_broker: lapin::Connection,
    job_queue: tokio::sync::Mutex<lapin::Consumer>,
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
        let message_broker = lapin::Connection::connect(&amqp_url, Default::default()).await?;
        let channel = message_broker.create_channel().await?;
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
            message_broker,
            job_queue: tokio::sync::Mutex::new(job_queue),
            jobs: DashMap::new(),
        })
    }
}

#[tonic::async_trait]
impl proto::dagyo_sidecar_server::DagyoSidecar for Sidecar {
    type GetInputStreamStream =
        Box<dyn Stream<Item = Result<InputStreamElement, Status>> + Send + 'static + Unpin>;

    async fn take_job(
        &self,
        request: Request<Streaming<Timestamp>>,
    ) -> Result<Response<proto::Job>, Status> {
        let raw_job: Delivery = self
            .job_queue
            .lock()
            .await
            .next()
            .await
            .ok_or_else(|| Status::new(tonic::Code::Internal, "No job available"))?
            .map_err(|e| {
                Status::new(
                    tonic::Code::Internal,
                    format!("Failed to get job from queue: {}", e),
                )
            })?;

        let desc: JobDesc = serde_json::from_slice(&raw_job.data)
            .map_err(|e| Status::new(tonic::Code::Internal, "Failed to parse job description"))?;

        unimplemented!()
    }

    async fn get_input_stream(
        &self,
        request: Request<StreamId>,
    ) -> Result<Response<Self::GetInputStreamStream>, Status> {
        unimplemented!()
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

    async fn wait_for_stop(&self, request: Request<proto::Job>) -> Result<Response<()>, Status> {
        unimplemented!()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let host = "[::1]:50051".parse()?;
    let sidecar = Sidecar::init().await?;
    tonic::transport::Server::builder()
        .add_service(DagyoSidecarServer::new(sidecar))
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
