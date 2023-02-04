import common
from common import asyncmain, eprint
import time


@asyncmain
async def main() -> None:
    print("Hello, world!")
    time.sleep(10000)
    async for job in common.jobs():
        eprint(job)
