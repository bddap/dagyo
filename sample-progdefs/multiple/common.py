import typing as T
from dataclasses import dataclass
import os
import asyncio
from aio_pika import connect
from aio_pika.abc import AbstractIncomingMessage
from functools import lru_cache


@lru_cache()
async def connect_mq():
    que = os.environ["DAGYO_QUEUE"]  # e.g. "amqp://guest:guest@localhost/"
    return await connect(que)


@dataclass
class Message:
    messages: T.Optional[list[bytes]]


class InStream:
    pass


class OutStream:
    pass


@dataclass
class Job:
    inputs: dict[str, InStream]
    outputs: dict[str, OutStream]
    stop: InStream
    failure: OutStream
    health: OutStream


def await_job() -> Job:

    pass


async def on_message(message: AbstractIncomingMessage) -> None:
    async with message.process():
        print(f" [x] Received message {message!r}")
        print(f"     Message body is: {message.body!r}")


async def amain() -> None:
    job_mailbox = os.environ["DAGYO_JOB"]
    connection = await connect_mq()

    async with connection:
        channel = await connection.channel()
        await channel.set_qos(prefetch_count=1)
        queue = await channel.declare_queue(job_mailbox)
        await queue.consume(on_message)
        await asyncio.Future()


def main() -> None:
    asyncio.run(amain())


if __name__ == "__main__":
    main()
