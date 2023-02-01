FROM python:3.10

ARG SCRIPT

RUN mkdir /app
WORKDIR /app

COPY pyproject.toml .
COPY poetry.lock .
RUN pip3 install poetry \
	&& poetry config virtualenvs.create false \
	&& poetry install --no-dev

COPY . .
RUN [ -f $SCRIPT ]

ENTRYPOINT python $SCRIPT
