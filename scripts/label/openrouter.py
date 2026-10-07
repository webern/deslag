"""The OpenRouter side of the labelling runner: the request body, the transport, the reply.

Standard library only. The key is read from `OPENROUTER_API_KEY` when a call is made, stripped and
checked to be one token of printable characters, goes into the `Authorization` header and nowhere
else: it is not printed, logged, saved with a request or put in an error, and a key that fails the
check is refused without being shown. Every request pins one endpoint of one provider, by the tag
the endpoint listing gives it, with fallbacks off, and asks for the parameters it sends to be
honoured; a reply from any other provider or model is an error, since a run's provenance names the
provider and model that made it.
"""

import email.utils
import http.client
import json
import os
import random
import re
import socket
import time
import urllib.error
import urllib.request

API = "https://openrouter.ai/api/v1"
KEY_VARIABLE = "OPENROUTER_API_KEY"

# The reply was not JSON: a gateway page or half a body, which an endpoint sent, so it is its own.
NOT_JSON = "the reply was not JSON"

# HTTP statuses worth asking again for: the request timed out, or the service was busy or down. 520 to
# 524 are the gateway's (Cloudflare's) own, and 529 is "overloaded".
RETRYABLE = {408, 425, 429, 500, 502, 503, 504, 520, 521, 522, 523, 524, 529}

# Quantisations from least to most precise; those in one tier count as the same.
QUANT_RANK = {"int4": 0, "fp4": 0, "fp6": 1, "int8": 2, "fp8": 2, "bf16": 3, "fp16": 3, "fp32": 4}


class ApiError(Exception):
    """A call that failed for a reason asking again will not fix. Never holds the key."""


class ProviderMismatch(ApiError):
    """A reply from a provider other than the one pinned."""


class CutOff(ApiError):
    """A reply cut off at max_tokens: a bad reply, which the caller asks again in smaller pieces."""


class ProviderRefused(ApiError):
    """The provider refused the request: an error body in place of a completion, or a reply whose
    finish reason is `content_filter`. It belongs to the endpoint, so another endpoint may answer."""


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


def listed_quantization(listing, tag):
    """The quantisation the listing gives the endpoint tagged `tag`, or None."""
    endpoints = listing.get("data", listing).get("endpoints", [])
    return next((endpoint.get("quantization") for endpoint in endpoints if endpoint.get("tag") == tag), None)


def weakest(quantizations):
    """The least precise of a list of quantisations, or None if the list is empty or not known."""
    known = [q for q in quantizations or [] if q in QUANT_RANK]
    return min(known, key=QUANT_RANK.get, default=None)


def pinned_endpoint(listing, config, at_least=None):
    """The endpoint of `listing` (the models/<id>/endpoints JSON) that `config` pins, checked.

    Raises ApiError when the listing has no such endpoint, when its quantisation is not one the
    config allows, or when it does not support a parameter the body will send. With `at_least`, a
    quantisation, the endpoint's must be that or a more precise one, and the config's own
    `quantizations` is not looked at: this is how an alternative endpoint is checked. A floor that is
    not a known quantisation (`unknown`, say) asks for nothing.
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
    if at_least in QUANT_RANK:
        have = QUANT_RANK.get(endpoint.get("quantization"))
        if have is None or have < QUANT_RANK[at_least]:
            raise ApiError(
                f"{config['model']} at `{config['provider']}` is {endpoint.get('quantization')}, "
                f"which is not {at_least} or better"
            )
    elif at_least is None and allowed and endpoint.get("quantization") not in allowed:
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

    def __init__(self, content, raw_content, provider, usage, cost, finish, think, ident,
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
        self.finish = finish
        self.think = think
        self.ident = ident


def parse_reply(response):
    """A Reply from a chat completion's JSON. Its cost is the usage's own, and None when the reply
    gives none, which the ledger books at the call's worst case."""
    if "error" in response and not response.get("choices"):
        error = response["error"]
        message = error.get("message") if isinstance(error, dict) else error
        raise ProviderRefused(f"the API answered with an error: {message}")
    choices = response.get("choices") or []
    if not choices:
        raise ApiError("the reply has no choices")
    choice = choices[0]
    message = choice.get("message") or {}
    raw = message.get("content") or ""
    content, think = strip_think(raw)
    usage = response.get("usage") or {}
    cost = usage.get("cost")
    return Reply(
        content, raw, response.get("provider"), usage, None if cost is None else float(cost),
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


class _Refuse(urllib.request.HTTPRedirectHandler):
    """Follows no redirect: urllib would send the same headers, the key's among them, to wherever the
    reply points."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ApiError(f"the server answered HTTP {code}, a redirect, which is not followed so that the key is never sent elsewhere")


_OPENER = urllib.request.build_opener(_Refuse)


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
            with _OPENER.open(request, timeout=timeout) as handle:
                return json.loads(handle.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            text = error.read().decode("utf-8", "replace")[:500]
            if error.code in RETRYABLE:
                raise Retryable(f"HTTP {error.code}", retry_after(error.headers), error.code) from None
            raise ApiError(f"HTTP {error.code}: {text}") from None
        except (socket.timeout, TimeoutError):
            raise Retryable("timeout") from None
        except urllib.error.URLError as error:
            if isinstance(error.reason, (socket.timeout, TimeoutError)):
                raise Retryable("timeout") from None
            raise Retryable(f"could not connect, {type(error.reason).__name__}") from None
        except (http.client.HTTPException, OSError) as error:
            # A connection reset or a reply cut short: what was sent may have been billed, and the
            # ledger still has it booked at its worst case. Only the type is kept, never the text.
            raise Retryable(type(error).__name__) from None
        except (json.JSONDecodeError, UnicodeDecodeError):
            raise Retryable(NOT_JSON) from None


class Retryable(Exception):
    """A failure that asking again may fix: a timeout, a busy service, a rate limit. Its text is a
    status or an exception type, safe to print. `after` is the wait in seconds the server asked for,
    and `status` the HTTP status, if there was one (read from a reason `HTTP 429` if not given).

    `owned` says whose failure it is. An answer from the endpoint, HTTP 429 or 5xx, or a reply that
    is not JSON, is the endpoint's own, and another endpoint of the model may do better. Nothing
    else is: a connection refused, a name that does not resolve, a timeout with no answer, a dropped
    connection and a 408 or 425 are as likely to be the network here, or OpenRouter, as the endpoint,
    and every endpoint would fail the same way."""

    def __init__(self, reason, after=None, status=None):
        super().__init__(reason)
        self.after = after
        if status is None:
            found = re.fullmatch(r"HTTP (\d{3})", str(reason))
            status = int(found.group(1)) if found else None
        self.status = status

    @property
    def owned(self):
        if self.status is not None:
            return self.status == 429 or self.status >= 500
        return str(self) == NOT_JSON


class RetriesExhausted(ApiError):
    """A call still failing after every wait it was allowed (a rate limit, a server error, a timeout, a
    dropped connection). `status` is that of the last failure, if it had one; `reason` is its text, a
    status or an exception type. `owned` is [Retryable.owned] of the last failure: whether it was the
    endpoint's own."""

    def __init__(self, message, status=None, reason="", owned=False):
        super().__init__(message)
        self.status = status
        self.reason = reason
        self.owned = owned


def retry_after(headers, now=time.time):
    """Seconds the server asks us to wait, from `Retry-After` (seconds or an HTTP date) or
    `X-RateLimit-Reset` (a Unix time, in seconds or milliseconds, or seconds from now), or None."""
    if headers is None:
        return None
    given = headers.get("Retry-After")
    if given:
        try:
            return max(0.0, float(given))
        except ValueError:
            try:
                return max(0.0, email.utils.parsedate_to_datetime(given).timestamp() - now())
            except (TypeError, ValueError):
                pass
    reset = headers.get("X-RateLimit-Reset")
    if reset:
        try:
            value = float(reset)
        except ValueError:
            return None
        if value > 1e11:
            value /= 1000.0
        return max(0.0, value - now()) if value > 1e8 else max(0.0, value)
    return None


def with_retries(call, attempts, sleep=time.sleep, base=5.0, max_wait=600.0, longest=120.0,
                 rng=random.random, on_retry=None):
    """`call()`, asked again after a Retryable, up to `attempts` times in all, as long as the waits
    add up to `max_wait` seconds. A wait is `base` doubled each time, at most `longest`, with jitter
    (half to all of it), or what the server asked for, with up to a second of jitter, if that is
    longer. `on_retry(attempt, attempts, reason, wait)` is told of each wait before it. Returns
    (result, how many asks were repeated)."""
    waited = 0.0
    for attempt in range(attempts):
        try:
            return call(), attempt
        except Retryable as error:
            if attempt + 1 == attempts:
                raise RetriesExhausted(
                    f"{error}, after {attempts} attempts and {waited:.0f} s of waiting", error.status, str(error), error.owned
                ) from None
            wait = min(longest, base * (2 ** attempt))
            wait = wait * (0.5 + 0.5 * rng())
            if error.after is not None:
                wait = max(wait, error.after + rng())
            if waited + wait > max_wait:
                raise RetriesExhausted(
                    f"{error}, after {attempt + 1} attempts and {waited:.0f} s of waiting, the most allowed",
                    error.status,
                    str(error),
                    error.owned,
                ) from None
            if on_retry:
                on_retry(attempt + 1, attempts, str(error), wait)
            sleep(wait)
            waited += wait
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
