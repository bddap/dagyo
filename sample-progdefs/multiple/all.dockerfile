FROM python:3.10-slim-bullseye AS builder

RUN mkdir /app
WORKDIR /app

RUN pip3 install poetry \
	&& poetry config virtualenvs.create false

COPY pyproject.toml .
COPY poetry.lock .

RUN poetry install --no-dev \
	&& poetry export -f requirements.txt >> requirements.txt

FROM python:3.10-slim-bullseye AS runtime

RUN mkdir /app

COPY --from=builder /app/requirements.txt /app
RUN pip install --no-cache-dir -r /app/requirements.txt

COPY . .

ARG SCRIPT
RUN mv $SCRIPT ./entry.py

CMD ["python", "./entry.py"]
