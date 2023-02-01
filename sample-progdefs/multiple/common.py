import typing as T
from dataclasses import dataclass
import os
from aio_pika import connect, Message
from aio_pika.abc import AbstractIncomingMessage, AbstractQueue, AbstractChannel
from functools import lru_cache
from pydantic import BaseModel
from dataclasses import dataclass
import sys
import asyncio


def eprint(*args, **kwargs) -> None:
    print(*args, file=sys.stderr, **kwargs)


@lru_cache()
async def connect_mq():
    que = os.environ["DAGYO_QUEUE"]  # e.g. "amqp://guest:guest@localhost/"
    return await connect(que)


InStreamAddr: T.TypeAlias = str
OutStreamAddr: T.TypeAlias = str
InStream: T.TypeAlias = AbstractQueue


@dataclass
class OutStream:
    channel: AbstractChannel
    addr: OutStreamAddr

    async def send(self, msg: bytes):
        await self.channel.default_exchange.publish(Message(msg), routing_key=self.addr)


class JobDesc(BaseModel):
    """Serialized Job"""

    inputs: dict[str, InStreamAddr]
    outputs: dict[str, OutStreamAddr]
    panic: OutStreamAddr
    health: OutStreamAddr
    stop: InStreamAddr


@dataclass
class Job:
    # the message that encoded this job
    original_message: AbstractIncomingMessage
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
    async def create(
        message: AbstractIncomingMessage, channel: AbstractChannel
    ) -> "Job":
        jd = JobDesc.parse_raw(message.body)
        return Job(
            original_message=message,
            inputs={k: await channel.get_queue(v) for k, v in jd.inputs.items()},
            outputs={k: OutStream(channel, v) for k, v in jd.outputs.items()},
            panic=OutStream(channel, jd.panic),
            health=OutStream(channel, jd.health),
            stop=await channel.get_queue(jd.stop),
        )

    async def __aenter__(self) -> "Job":
        return self

    async def __aexit__(
        self, exc_type: T.Type[BaseException], exc_value: BaseException, traceback
    ) -> bool:
        print(exc_type, exc_value, traceback)
        assert exc_type is None == exc_value is None
        assert False, """
        this should:
          do_panic in error
          ack original_message on success
          close all outputs
          assert all inputs have been closed?
          finish closing all inputs?
          post to health on success?
        """
        # Is there any way we can assert the stop signal was heeded?
        # Maybe stop should result in an exception?

        # Can we panic with the full traceback? Maybe panics should be json.
        return True


async def jobs() -> T.AsyncIterator[Job]:
    job_mailbox = os.environ["DAGYO_JOBS"]
    connection = await connect_mq()

    async with connection:
        channel = await connection.channel()
        await channel.set_qos(prefetch_count=1)
        queue = await channel.get_queue(job_mailbox)
        async for message in queue.iterator():
            yield await Job.create(message, channel)


def asyncmain(func: T.Callable[[], T.Coroutine]) -> None:
    """
    Decorator that runs an async function using asyncio only if the function's
    containing module is "__main__".
    """
    if func.__module__ == "__main__":
        asyncio.run(func())
