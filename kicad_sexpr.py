"""Minimal S-expression reader for KiCad PCB files."""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Union


SExpr = Union[str, List["SExpr"]]


@dataclass
class _TokenStream:
    tokens: list[str]
    index: int = 0

    def peek(self) -> str | None:
        if self.index >= len(self.tokens):
            return None
        return self.tokens[self.index]

    def pop(self) -> str:
        token = self.peek()
        if token is None:
            raise ValueError("unexpected end of input")
        self.index += 1
        return token


def _tokenize(text: str) -> list[str]:
    tokens: list[str] = []
    index = 0

    while index < len(text):
        char = text[index]

        if char.isspace():
            index += 1
            continue

        if char == ";":
            while index < len(text) and text[index] != "\n":
                index += 1
            continue

        if char in ("(", ")"):
            tokens.append(char)
            index += 1
            continue

        if char == '"':
            index += 1
            value_chars: list[str] = []
            while index < len(text):
                current = text[index]
                if current == "\\" and (index + 1) < len(text):
                    value_chars.append(text[index + 1])
                    index += 2
                    continue
                if current == '"':
                    index += 1
                    break
                value_chars.append(current)
                index += 1
            else:
                raise ValueError("unterminated string in S-expression")

            tokens.append("".join(value_chars))
            continue

        start = index
        while index < len(text) and (not text[index].isspace()) and text[index] not in ('(', ')'):
            index += 1
        tokens.append(text[start:index])

    return tokens


def _parse_expr(stream: _TokenStream) -> SExpr:
    token = stream.pop()

    if token == "(":
        result: list[SExpr] = []
        while True:
            next_token = stream.peek()
            if next_token is None:
                raise ValueError("unterminated list in S-expression")
            if next_token == ")":
                stream.pop()
                return result
            result.append(_parse_expr(stream))

    if token == ")":
        raise ValueError("unexpected ')' in S-expression")

    return token


def parse_sexpr(text: str) -> SExpr:
    stream = _TokenStream(_tokenize(text))
    result = _parse_expr(stream)
    if stream.peek() is not None:
        raise ValueError("extra tokens after root S-expression")
    return result
