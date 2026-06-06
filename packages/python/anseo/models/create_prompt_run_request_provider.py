from enum import Enum


class CreatePromptRunRequestProvider(str, Enum):
    ANTHROPIC = "anthropic"
    GEMINI = "gemini"
    GROK = "grok"
    MISTRAL = "mistral"
    MOCK = "mock"
    OPENAI = "openai"
    OPENROUTER = "openrouter"
    PERPLEXITY = "perplexity"

    def __str__(self) -> str:
        return str(self.value)
