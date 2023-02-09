import common
from common import eprint, OutStream, InStream, blastoff
from dataclasses import dataclass
import asyncio

# This progdef is stateful, and state grows with every message. We protect against
# OOM by limiting the number the total size of all inputs.
MAX_BYTES_PER_JOB = 10_000_000


@dataclass
class Inputs:
    a: InStream
    b: InStream


@dataclass
class Outputs:
    product: OutStream


async def run(job: common.Job) -> None:
    inputs = Inputs(**job.inputs)
    outputs = Outputs(**job.outputs)

    total_bytes = 0

    ays: list[str] = []
    bes: list[str] = []
    # this lock guards both ays and bes
    lock = asyncio.Lock()

    # When an `a` arrives combine with all the `bs` we've seen so far
    # When a `b` arrives combine with all the `as` we've seen so far
    # When both a and b streams close, return. The output stream will be closed automatically

    def inc_total(n: int):
        nonlocal total_bytes
        total_bytes += n
        if total_bytes > MAX_BYTES_PER_JOB:
            raise Exception(
                f"unordered_cartesian_product recieved more than {MAX_BYTES_PER_JOB} bytes"
            )

    async def consume_ays():
        async for message in inputs.a:
            inc_total(len(message))
            s = message.decode("utf-8")
            async with lock:
                ays.append(s)
                for b in bes:
                    await outputs.product.send((s + b).encode("utf-8"))

    async def consume_bes():
        async for message in inputs.b:
            inc_total(len(message))
            s = message.decode("utf-8")
            async with lock:
                bes.append(s)
                for a in ays:
                    await outputs.product.send((a + s).encode("utf-8"))

    await asyncio.gather(consume_ays(), consume_bes())


if __name__ == "__main__":
    eprint("starting unordered_cartesian_product")
    blastoff(run, 10)
