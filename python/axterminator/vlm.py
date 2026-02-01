"""Visual Language Model integration for element detection.

This module provides VLM-based element detection as a fallback strategy
when traditional locators fail. Supports multiple backends:

- MLX (local, fast, private) - DEFAULT
- Anthropic Claude Vision API
- OpenAI GPT-4V API
- Google Gemini Vision API

Usage:
    from axterminator.vlm import VLMDetector, configure_vlm

    # Use local MLX (default)
    configure_vlm(backend="mlx")

    # Or use Claude Vision
    configure_vlm(backend="anthropic", api_key="sk-...")

    # Or use Gemini
    configure_vlm(backend="gemini", api_key="...")

    # The detector is automatically used by the healing system
"""

from __future__ import annotations

import base64
import json
import os
import subprocess
import tempfile
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Tuple

# Global VLM detector instance
_vlm_detector: Optional["VLMDetector"] = None


@dataclass
class BoundingBox:
    """Bounding box for detected element."""

    x: float
    y: float
    width: float
    height: float

    @property
    def center(self) -> Tuple[float, float]:
        """Get center point of bounding box."""
        return (self.x + self.width / 2, self.y + self.height / 2)


class VLMBackend(ABC):
    """Abstract base class for VLM backends."""

    @abstractmethod
    def detect_element(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Detect element matching description in image.

        Args:
            image_data: PNG image data
            description: Natural language description of element to find
            image_width: Width of the image in pixels
            image_height: Height of the image in pixels

        Returns:
            BoundingBox if element found, None otherwise
        """
        pass


class MLXBackend(VLMBackend):
    """Local MLX backend using Qwen2-VL or Florence-2."""

    def __init__(self, model: str = "mlx-community/Qwen2-VL-2B-Instruct-4bit"):
        self.model = model
        self._model_loaded = False
        self._generate = None
        self._processor = None

    def _ensure_model_loaded(self):
        """Lazy load the MLX model."""
        if self._model_loaded:
            return

        try:
            from mlx_vlm import generate, load

            model, processor = load(self.model)
            self._model = model
            self._processor = processor
            self._generate = generate
            self._model_loaded = True
        except ImportError:
            raise ImportError(
                "MLX VLM not installed. Install with: pip install mlx-vlm"
            )

    def detect_element(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Detect element using local MLX model."""
        self._ensure_model_loaded()

        # Save image to temp file (mlx-vlm needs file path)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(image_data)
            temp_path = f.name

        try:
            prompt = f"""Look at this macOS application screenshot.
Find the UI element that matches this description: "{description}"

Return ONLY a JSON object with the bounding box coordinates as percentages (0-100) of the image dimensions:
{{"x": <left_percent>, "y": <top_percent>, "width": <width_percent>, "height": <height_percent>}}

If the element is not found, return: {{"error": "not found"}}"""

            output = self._generate(
                self._model,
                self._processor,
                temp_path,
                prompt,
                max_tokens=100,
            )

            # Parse JSON from output
            return self._parse_bbox_response(output, image_width, image_height)

        finally:
            os.unlink(temp_path)

    def _parse_bbox_response(
        self, response: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Parse bounding box from model response."""
        try:
            # Find JSON in response
            start = response.find("{")
            end = response.rfind("}") + 1
            if start == -1 or end == 0:
                return None

            data = json.loads(response[start:end])

            if "error" in data:
                return None

            # Convert percentages to pixels
            return BoundingBox(
                x=data["x"] * image_width / 100,
                y=data["y"] * image_height / 100,
                width=data["width"] * image_width / 100,
                height=data["height"] * image_height / 100,
            )
        except (json.JSONDecodeError, KeyError, TypeError):
            return None


class AnthropicBackend(VLMBackend):
    """Anthropic Claude Vision API backend."""

    def __init__(self, api_key: Optional[str] = None, model: str = "claude-sonnet-4-20250514"):
        self.api_key = api_key or os.environ.get("ANTHROPIC_API_KEY")
        self.model = model
        if not self.api_key:
            raise ValueError(
                "Anthropic API key required. Set ANTHROPIC_API_KEY or pass api_key."
            )

    def detect_element(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Detect element using Claude Vision."""
        try:
            import anthropic
        except ImportError:
            raise ImportError(
                "Anthropic SDK not installed. Install with: pip install anthropic"
            )

        client = anthropic.Anthropic(api_key=self.api_key)

        # Encode image as base64
        image_b64 = base64.b64encode(image_data).decode("utf-8")

        prompt = f"""Look at this macOS application screenshot.
Find the UI element that matches this description: "{description}"

Return ONLY a JSON object with the bounding box coordinates as percentages (0-100) of the image dimensions:
{{"x": <left_percent>, "y": <top_percent>, "width": <width_percent>, "height": <height_percent>}}

If the element is not found, return: {{"error": "not found"}}"""

        response = client.messages.create(
            model=self.model,
            max_tokens=100,
            messages=[
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": image_b64,
                            },
                        },
                        {"type": "text", "text": prompt},
                    ],
                }
            ],
        )

        return self._parse_bbox_response(
            response.content[0].text, image_width, image_height
        )

    def _parse_bbox_response(
        self, response: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Parse bounding box from model response."""
        try:
            start = response.find("{")
            end = response.rfind("}") + 1
            if start == -1 or end == 0:
                return None

            data = json.loads(response[start:end])

            if "error" in data:
                return None

            return BoundingBox(
                x=data["x"] * image_width / 100,
                y=data["y"] * image_height / 100,
                width=data["width"] * image_width / 100,
                height=data["height"] * image_height / 100,
            )
        except (json.JSONDecodeError, KeyError, TypeError):
            return None


class GeminiBackend(VLMBackend):
    """Google Gemini Vision API backend."""

    def __init__(self, api_key: Optional[str] = None, model: str = "gemini-2.0-flash"):
        self.api_key = api_key or os.environ.get("GOOGLE_API_KEY")
        self.model = model
        if not self.api_key:
            raise ValueError(
                "Google API key required. Set GOOGLE_API_KEY or pass api_key."
            )

    def detect_element(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Detect element using Gemini Vision."""
        try:
            import google.generativeai as genai
        except ImportError:
            raise ImportError(
                "Google Generative AI SDK not installed. Install with: pip install google-generativeai"
            )

        genai.configure(api_key=self.api_key)
        model = genai.GenerativeModel(self.model)

        # Create image part from bytes
        image_part = {"mime_type": "image/png", "data": image_data}

        prompt = f"""Look at this macOS application screenshot.
Find the UI element that matches this description: "{description}"

Return ONLY a JSON object with the bounding box coordinates as percentages (0-100) of the image dimensions:
{{"x": <left_percent>, "y": <top_percent>, "width": <width_percent>, "height": <height_percent>}}

If the element is not found, return: {{"error": "not found"}}"""

        response = model.generate_content([image_part, prompt])

        return self._parse_bbox_response(
            response.text, image_width, image_height
        )

    def _parse_bbox_response(
        self, response: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Parse bounding box from model response."""
        try:
            start = response.find("{")
            end = response.rfind("}") + 1
            if start == -1 or end == 0:
                return None

            data = json.loads(response[start:end])

            if "error" in data:
                return None

            return BoundingBox(
                x=data["x"] * image_width / 100,
                y=data["y"] * image_height / 100,
                width=data["width"] * image_width / 100,
                height=data["height"] * image_height / 100,
            )
        except (json.JSONDecodeError, KeyError, TypeError):
            return None


class OpenAIBackend(VLMBackend):
    """OpenAI GPT-4V API backend."""

    def __init__(self, api_key: Optional[str] = None, model: str = "gpt-4o"):
        self.api_key = api_key or os.environ.get("OPENAI_API_KEY")
        self.model = model
        if not self.api_key:
            raise ValueError(
                "OpenAI API key required. Set OPENAI_API_KEY or pass api_key."
            )

    def detect_element(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Detect element using GPT-4V."""
        try:
            import openai
        except ImportError:
            raise ImportError(
                "OpenAI SDK not installed. Install with: pip install openai"
            )

        client = openai.OpenAI(api_key=self.api_key)

        # Encode image as base64
        image_b64 = base64.b64encode(image_data).decode("utf-8")

        prompt = f"""Look at this macOS application screenshot.
Find the UI element that matches this description: "{description}"

Return ONLY a JSON object with the bounding box coordinates as percentages (0-100) of the image dimensions:
{{"x": <left_percent>, "y": <top_percent>, "width": <width_percent>, "height": <height_percent>}}

If the element is not found, return: {{"error": "not found"}}"""

        response = client.chat.completions.create(
            model=self.model,
            max_tokens=100,
            messages=[
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": f"data:image/png;base64,{image_b64}",
                            },
                        },
                        {"type": "text", "text": prompt},
                    ],
                }
            ],
        )

        return self._parse_bbox_response(
            response.choices[0].message.content, image_width, image_height
        )

    def _parse_bbox_response(
        self, response: str, image_width: int, image_height: int
    ) -> Optional[BoundingBox]:
        """Parse bounding box from model response."""
        try:
            start = response.find("{")
            end = response.rfind("}") + 1
            if start == -1 or end == 0:
                return None

            data = json.loads(response[start:end])

            if "error" in data:
                return None

            return BoundingBox(
                x=data["x"] * image_width / 100,
                y=data["y"] * image_height / 100,
                width=data["width"] * image_width / 100,
                height=data["height"] * image_height / 100,
            )
        except (json.JSONDecodeError, KeyError, TypeError):
            return None


class VLMDetector:
    """Main VLM detector class used by the healing system."""

    def __init__(self, backend: VLMBackend):
        self.backend = backend

    def detect(
        self, image_data: bytes, description: str, image_width: int, image_height: int
    ) -> Optional[Tuple[float, float]]:
        """Detect element and return center coordinates.

        Args:
            image_data: PNG image data
            description: Natural language description of element
            image_width: Width of image in pixels
            image_height: Height of image in pixels

        Returns:
            (x, y) center coordinates if found, None otherwise
        """
        bbox = self.backend.detect_element(
            image_data, description, image_width, image_height
        )
        if bbox:
            return bbox.center
        return None


def configure_vlm(
    backend: str = "mlx",
    model: Optional[str] = None,
    api_key: Optional[str] = None,
) -> None:
    """Configure the global VLM detector.

    Args:
        backend: One of "mlx", "anthropic", "openai"
        model: Model name (backend-specific)
        api_key: API key for cloud backends

    Examples:
        # Use local MLX (default, fast, private)
        configure_vlm(backend="mlx")

        # Use Claude Vision
        configure_vlm(backend="anthropic", api_key="sk-ant-...")

        # Use GPT-4V
        configure_vlm(backend="openai", api_key="sk-...")

        # Use specific MLX model
        configure_vlm(backend="mlx", model="mlx-community/Florence-2-base-4bit")
    """
    global _vlm_detector

    if backend == "mlx":
        vlm_backend = MLXBackend(model=model or "mlx-community/Qwen2-VL-2B-Instruct-4bit")
    elif backend == "anthropic":
        vlm_backend = AnthropicBackend(api_key=api_key, model=model or "claude-sonnet-4-20250514")
    elif backend == "openai":
        vlm_backend = OpenAIBackend(api_key=api_key, model=model or "gpt-4o")
    elif backend == "gemini":
        vlm_backend = GeminiBackend(api_key=api_key, model=model or "gemini-2.0-flash")
    else:
        raise ValueError(f"Unknown backend: {backend}. Use 'mlx', 'anthropic', 'openai', or 'gemini'")

    _vlm_detector = VLMDetector(vlm_backend)


def get_vlm_detector() -> Optional[VLMDetector]:
    """Get the global VLM detector instance."""
    return _vlm_detector


def detect_element_visual(
    image_data: bytes, description: str, image_width: int, image_height: int
) -> Optional[Tuple[float, float]]:
    """Detect element visually using configured VLM.

    This function is called from Rust when visual_vlm strategy is used.

    Args:
        image_data: PNG screenshot data
        description: Element description to find
        image_width: Image width in pixels
        image_height: Image height in pixels

    Returns:
        (x, y) center coordinates if found, None otherwise
    """
    global _vlm_detector

    # Auto-configure with MLX if not configured
    if _vlm_detector is None:
        try:
            configure_vlm(backend="mlx")
        except ImportError:
            # MLX not available, try to fallback to environment-configured backend
            if os.environ.get("ANTHROPIC_API_KEY"):
                configure_vlm(backend="anthropic")
            elif os.environ.get("OPENAI_API_KEY"):
                configure_vlm(backend="openai")
            elif os.environ.get("GOOGLE_API_KEY"):
                configure_vlm(backend="gemini")
            else:
                return None

    if _vlm_detector is None:
        return None

    return _vlm_detector.detect(image_data, description, image_width, image_height)
