import common
from common import asyncmain, eprint, OutStream, InStream, Job
from dataclasses import dataclass
from asyncio import Semaphore
import asyncio


@dataclass
class Inputs:
    name: InStream


@dataclass
class Outputs:
    greeting: OutStream


async def run(job: common.Job) -> None:
    inputs = Inputs(**job.inputs)
    outputs = Outputs(**job.outputs)
    async for message in inputs.name:
        # convert message to a string, panic if it's not utf-8
        name = message.decode("utf-8")
        eprint("got name", name)
        await outputs.greeting.send(f"hello {name}".encode("utf-8"))


# run job and then release the lock
async def run_lock(job: Job, sem: Semaphore) -> None:
    try:
        async with job:
            await run(job)
    finally:
        sem.release()


# we actually want to handle some number of jobs at a time
@asyncmain
async def main() -> None:
    eprint("Greet Starting..")

    # how many streams will this executor accept at a time
    worker_slots = Semaphore(1_000)

    jobs = common.jobs()

    while True:
        await worker_slots.acquire()
        try:
            job = await anext(jobs)
        except StopAsyncIteration:
            break
        asyncio.create_task(run_lock(job, worker_slots))
