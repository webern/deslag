"""The OpenRouter side of the labelling runner: the request body, the transport, the reply.

Standard library only. The key is read from `OPENROUTER_API_KEY` when a call is made, stripped and
checked to be one token of printable characters, goes into the `Authorization` header and nowhere
else: it is not printed, logged, saved with a request or put in an error, and a key that fails the
check is refused without being shown. Every request pins one endpoint of one provider, by the tag
the endpoint listing gives it, with fallbacks off, and asks for the parameters it sends to be
honoured; a reply from any other provider or model is an error, since a run's provenance names the
provider and model that made it.
"""

import http.client
import json
import os
import re
import socket
import time
import urllib.error
import urllib.request

API = "https://openrouter.ai/api/v1"
KEY_VARIABLE = "OPENROUTER_API_KEY"

# HTTP statuses worth asking again for: the request timed out, or the service was busy or down.
RETRYABLE = {408, 425, 429, 500, 502, 503, 504}


class ApiError(Exception):
    """A call that failed for a reason asking again will not fix. Never holds the key."""


class ProviderMismatch(ApiError):
    """A reply from a provider other than the one pinned."""


def endpoints_url(model):
    return f"{API}/models/{model}/endpoints"


def request_body(config, system, user):
    """The body of one chat completion for the model `config` pins, `system` and `user` as text."""
    provider = {
        "order": [config["provider"]],
        "allow_fallbacks": False,
        "require_parameters": True,
        "data_collection": "deny",
    }
    if config.get("quantizations"):
        provider["quantizations"] = list(config["quantizations"])
    body = {
        "model": config["model"],
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": config["max_tokens"],
        "provider": provider,
        "usage": {"include": True},
    }
    if config.get("temperature") is not None:
        body["temperature"] = config["temperature"]
    if config.get("reasoning") is not None:
        body["reasoning"] = config["reasoning"]
    return body


def parameters_sent(body):
    """The names of the optional parameters a body asks the provider to honour."""
    return [name for name in ("temperature", "reasoning", "max_tokens") if name in body]


def pinned_endpoint(listing, config):
    """The endpoint of `listing` (the models/<id>/endpoints JSON) that `config` pins, checked.

    Raises ApiError when the listing has no such endpoint, when its quantisation is not one the
    config allows, or when it does not support a parameter the body will send.
    """
    endpoints = listing.get("data", listing).get("endpoints", [])
    found = [endpoint for endpoint in endpoints if endpoint.get("tag") == config["provider"]]
    if not found:
        tags = ", ".join(sorted(str(endpoint.get("tag")) for endpoint in endpoints))
        raise ApiError(
            f"{config['model']} has no endpoint tagged `{config['provider']}`; the listing has: {tags}"
        )
    endpoint = found[0]
    allowed = config.get("quantizations")
    if allowed and endpoint.get("quantization") not in allowed:
        raise ApiError(
            f"{config['model']} at `{config['provider']}` is {endpoint.get('quantization')}, "
            f"and voters.json pins {', '.join(allowed)}"
        )
    sent = parameters_sent(request_body(config, "", ""))
    lacking = [name for name in sent if name not in endpoint.get("supported_parameters", [])]
    if lacking:
        raise ApiError(
            f"`{config['provider']}` for {config['model']} does not support {', '.join(lacking)}, "
            f"and the request requires every parameter it sends; set it to null in voters.json"
        )
    return endpoint


def prices(endpoint):
    """(USD per input token, USD per output token) of an endpoint of the listing."""
    pricing = endpoint["pricing"]
    return float(pricing["prompt"]), float(pricing["completion"])


_CLOSE_THINK = re.compile(r"</think(?:ing)?>", re.IGNORECASE)
_OPEN_THINK = re.compile(r"<think(?:ing)?>.*\Z", re.DOTALL | re.IGNORECASE)


def strip_think(text):
    """The text after the last `</think>`, which also drops a reasoning block whose opening tag the
    model's template left out, and any block left open at the end. Whether there was a block."""
    closes = list(_CLOSE_THINK.finditer(text))
    stripped = text[closes[-1].end():] if closes else text
    stripped = _OPEN_THINK.sub("", stripped)
    return stripped.strip(), stripped != text.strip()


class Reply:
    """What one call gave back, with what it cost."""

    def __init__(self, content, raw_content, provider, usage, cost, estimated, finish, think, ident,
                 model=None, fingerprint=None):
        self.content = content
        self.raw_content = raw_content
        self.provider = provider
        self.model = model
        self.fingerprint = fingerprint
        self.prompt_tokens = int(usage.get("prompt_tokens") or 0)
        self.completion_tokens = int(usage.get("completion_tokens") or 0)
        details = usage.get("completion_tokens_details") or {}
        self.reasoning_tokens = int(details.get("reasoning_tokens") or 0)
        self.cost = cost
        self.estimated = estimated
        self.finish = finish
        self.think = think
        self.ident = ident


def parse_reply(response, price_in, price_out):
    """A Reply from a chat completion's JSON. Cost is the usage's own; failing that, tokens at the
    endpoint's price, which is marked an estimate."""
    if "error" in response and not response.get("choices"):
        error = response["error"]
        message = error.get("message") if isinstance(error, dict) else error
        raise ApiError(f"the API answered with an error: {message}")
    choices = response.get("choices") or []
    if not choices:
        raise ApiError("the reply has no choices")
    choice = choices[0]
    message = choice.get("message") or {}
    raw = message.get("content") or ""
    content, think = strip_think(raw)
    usage = response.get("usage") or {}
    cost = usage.get("cost")
    estimated = cost is None
    if estimated and usage.get("prompt_tokens") is not None:
        cost = int(usage.get("prompt_tokens") or 0) * price_in + int(usage.get("completion_tokens") or 0) * price_out
    # No usage at all leaves the cost unknown (None): the ledger keeps the call booked at its worst.
    return Reply(
        content, raw, response.get("provider"), usage, None if cost is None else float(cost), estimated,
        choice.get("finish_reason"), think, response.get("id"), response.get("model"),
        response.get("system_fingerprint"),
    )


def check_provider(reply, endpoint, model=None):
    """Raises ProviderMismatch unless the reply came from the provider of the pinned endpoint and,
    with `model`, from that model: its name, or the name with a version after a `-` or `:`, as the
    API names a dated model. A reply that names no model cannot be checked and is refused."""
    check_names(reply.provider, reply.model, endpoint, model)


def check_names(provider, reply_model, endpoint, model=None):
    """The check of [check_provider] on a provider and a model as saved beside a reply."""
    wanted = str(endpoint.get("provider_name", "")).lower()
    got = str(provider or "").lower()
    if got != wanted:
        raise ProviderMismatch(
            f"the reply came from `{provider}`, and the pinned provider is `{endpoint.get('provider_name')}`"
        )
    if model is not None:
        named = str(reply_model or "")
        if not (named == model or named.startswith((model + "-", model + ":"))):
            raise ProviderMismatch(f"the reply names the model `{named}`, and the pinned model is `{model}`")


class Urllib:
    """The real transport. A transport has `get(url, timeout)` and `post(url, body, key, timeout)`,
    each returning the decoded JSON, and raises Retryable for what asking again may fix."""

    def get(self, url, timeout):
        request = urllib.request.Request(url, headers={"Accept": "application/json"})
        return self._send(request, timeout)

    def post(self, url, body, key, timeout):
        data = json.dumps(body).encode("utf-8")
        request = urllib.request.Request(
            url,
            data=data,
            headers={
                "Authorization": f"Bearer {key}",
                "Content-Type": "application/json",
                "X-Title": "deslag labelling",
            },
        )
        return self._send(request, timeout)

    @staticmethod
    def _send(request, timeout):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as handle:
                return json.loads(handle.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            text = error.read().decode("utf-8", "replace")[:500]
            if error.code in RETRYABLE:
                raise Retryable(f"HTTP {error.code}") from None
            raise ApiError(f"HTTP {error.code}: {text}") from None
        except (socket.timeout, TimeoutError):
            raise Retryable("the call timed out") from None
        except urllib.error.URLError as error:
            if isinstance(error.reason, (socket.timeout, TimeoutError)):
                raise Retryable("the call timed out") from None
            raise Retryable("could not connect") from None
        except (http.client.HTTPException, OSError):
            # A connection reset or a reply cut short: what was sent may have been billed, and the
            # ledger still has it booked at its worst case.
            raise Retryable("the connection failed") from None
        except (json.JSONDecodeError, UnicodeDecodeError):
            raise Retryable("the reply was not JSON") from None


class Retryable(Exception):
    """A failure that asking again may fix: a timeout, a busy service."""


def with_retries(call, attempts, sleep=time.sleep, base=2.0):
    """`call()`, asked again after a Retryable up to `attempts` times in all, with a doubling wait.
    Returns (result, how many asks were repeated)."""
    for attempt in range(attempts):
        try:
            return call(), attempt
        except Retryable as error:
            if attempt + 1 == attempts:
                raise ApiError(f"{error}, after {attempts} attempts") from None
            sleep(base * (2 ** attempt))
    raise AssertionError("unreachable")


def key():
    """The API key, from the environment, stripped, or ApiError. A key that is empty, or has
    whitespace or a control character inside it, or is not ASCII, is refused: an HTTP library puts
    the value in its error for a header it cannot send. Nothing here ever shows the value."""
    value = os.environ.get(KEY_VARIABLE, "").strip()
    if not value:
        raise ApiError(f"{KEY_VARIABLE} is not set")
    if not re.fullmatch(r"[\x21-\x7e]+", value):
        raise ApiError(
            f"{KEY_VARIABLE} has whitespace, a control character or a character that is not ASCII "
            f"inside it, which a key never has; set it again (its value is not shown)"
        )
    return value
