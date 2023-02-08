import typing as T
from types import TracebackType
from dataclasses import dataclass
import os
from aio_pika import connect, Message
from aio_pika.abc import AbstractQueue, AbstractChannel, AbstractExchange
from functools import lru_cache
from pydantic import BaseModel
from dataclasses import dataclass
import sys
import asyncio
from typing import Callable, Awaitable
from asyncio import Semaphore


def eprint(*args, **kwargs) -> None:
    print(*args, file=sys.stderr, **kwargs)


@lru_cache()
async def connect_mq():
    que = os.environ["DAGYO_QUEUE"]  # e.g. "amqp://guest:guest@localhost/"
    return await connect(que)


InStreamAddr: T.TypeAlias = str
OutStreamAddr: T.TypeAlias = str


@dataclass
class InStream:
    q: AbstractQueue

    @staticmethod
    async def create(channel: AbstractChannel, addr: InStreamAddr) -> "InStream":
        q = await channel.declare_queue(addr, passive=True)
        return InStream(q)

    async def __aiter__(self) -> T.AsyncIterator[bytes]:
        # A message starting with a single byte 0x01 indicates end of stream
        # an eos message must be exactly 1 byte long.
        # Any other message is data. The first byte is ignored.
        async for message in self.q.iterator():
            await message.ack()
            assert (
                len(message.body) >= 1
            ), "received an invalid message on and input stream, messages must be at least 1 byte long"
            if message.body[0] == 0:
                assert (
                    len(message.body) == 1
                ), "received an invalid end-of-stream message, eos messages must be exactly 1 byte long"
                break
            yield message.body[1:]


@dataclass
class OutStream:
    exchange: AbstractExchange
    addr: OutStreamAddr

    async def send(self, msg: bytes):
        await self.exchange.publish(Message(b"\0" + msg), routing_key=self.addr)

    async def close(self):
        await self.exchange.publish(Message(b"\1"), routing_key=self.addr)


class JobDesc(BaseModel):
    """Serialized Job"""

    inputs: dict[str, InStreamAddr]
    outputs: dict[str, OutStreamAddr]
    panic: OutStreamAddr
    health: OutStreamAddr
    stop: InStreamAddr


@dataclass
class Job:
    inputs: dict[str, InStream]
    outputs: dict[str, OutStream]
    panic: OutStream
    health: OutStream
    stop: InStream

    # report failure
    async def do_panic(self, note: str):
        await self.panic.send(note.encode())

    # send keepalive
    async def report_healthy(self):
        await self.health.send(b"")

    @staticmethod
    async def create(message_body: bytes, channel: AbstractChannel) -> "Job":
        jd = JobDesc.parse_raw(message_body)
        exchange = channel.default_exchange
        return Job(
            inputs={k: await InStream.create(channel, v) for k, v in jd.inputs.items()},
            outputs={k: OutStream(exchange, v) for k, v in jd.outputs.items()},
            panic=OutStream(exchange, jd.panic),
            health=OutStream(exchange, jd.health),
            stop=await InStream.create(channel, jd.stop),
        )

    async def __aenter__(self) -> "Job":
        return self

    async def __aexit__(
        self,
        exc_type: T.Optional[T.Type[BaseException]],
        exc_value: T.Optional[BaseException],
        traceback: T.Optional[TracebackType],
    ) -> bool:
        _ = exc_type, traceback

        if exc_value is not None:
            e = repr(exc_value)
            eprint("Paniking:", e)
            await self.do_panic(e)

        for to_close in [*self.outputs.values(), self.panic, self.health]:
            await to_close.close()

        # Is there any way we can assert the stop signal was heeded?
        # Maybe stop should result in an exception?
        # Can we panic with the full traceback? Maybe panics should be json.

        # returning True means we handled the exception
        return True


async def jobs() -> T.AsyncIterator[Job]:
    job_mailbox = os.environ["DAGYO_JOBS"]
    connection = await connect_mq()

    async with connection:
        jobs_channel = await connection.channel()
        await jobs_channel.set_qos(prefetch_count=1)
        jobs_queue = await jobs_channel.get_queue(job_mailbox)

        async for message in jobs_queue.iterator():
            # tell the queue never to give this job to another executor, even if we fail while running it
            await message.ack()

            streams_channel = await connection.channel()
            yield await Job.create(message.body, streams_channel)


def asyncmain(func: T.Callable[[], T.Coroutine]) -> None:
    """
    Decorator that runs an async function using asyncio only if the function's
    containing module is "__main__".
    """
    if func.__module__ == "__main__":
        asyncio.run(func())


def blastoff(cb: Callable[[Job], Awaitable[None]], worker_slots: int) -> None:
    """
    Run an async callback for each job, with a limit on the number of jobs running
    at once.
    Job outputs are automatically closed when the callback returns or raises.
    If the callback throws an error, a panic output is automatically sent.
    The callback is run within an asyncio executor.
    """

    work_permit = Semaphore(worker_slots)

    async def run_lock(job: Job) -> None:
        try:
            async with job:
                await cb(job)
        finally:
            work_permit.release()

    async def main() -> None:
        jbs = jobs()

        while True:
            await work_permit.acquire()
            try:
                job = await anext(jbs)
            except StopAsyncIteration:
                break
            asyncio.create_task(run_lock(job))

    asyncio.run(main())
