"""Tests for VLM visual element detection module."""

import pytest
from unittest.mock import MagicMock, patch
from axterminator.vlm import (
    BoundingBox,
    VLMDetector,
    VLMBackend,
    MLXBackend,
    AnthropicBackend,
    OpenAIBackend,
    GeminiBackend,
    configure_vlm,
    get_vlm_detector,
    detect_element_visual,
    _vlm_detector,
)


class TestBoundingBox:
    """Tests for BoundingBox dataclass."""

    def test_bounding_box_creation(self):
        """Test creating a bounding box."""
        bbox = BoundingBox(x=10, y=20, width=100, height=50)
        assert bbox.x == 10
        assert bbox.y == 20
        assert bbox.width == 100
        assert bbox.height == 50

    def test_bounding_box_center(self):
        """Test center calculation."""
        bbox = BoundingBox(x=0, y=0, width=100, height=100)
        assert bbox.center == (50.0, 50.0)

        bbox = BoundingBox(x=10, y=20, width=100, height=50)
        assert bbox.center == (60.0, 45.0)

    def test_bounding_box_zero_size(self):
        """Test center with zero-size box."""
        bbox = BoundingBox(x=50, y=50, width=0, height=0)
        assert bbox.center == (50.0, 50.0)


class TestMLXBackend:
    """Tests for MLX backend."""

    def test_mlx_backend_creation(self):
        """Test MLX backend can be created."""
        backend = MLXBackend()
        assert backend.model == "mlx-community/Qwen2-VL-2B-Instruct-4bit"
        assert not backend._model_loaded

    def test_mlx_backend_custom_model(self):
        """Test MLX backend with custom model."""
        backend = MLXBackend(model="custom/model")
        assert backend.model == "custom/model"

    def test_mlx_backend_parse_bbox_valid(self):
        """Test parsing valid bbox response."""
        backend = MLXBackend()
        response = '{"x": 10, "y": 20, "width": 30, "height": 40}'
        bbox = backend._parse_bbox_response(response, 100, 100)
        assert bbox is not None
        assert bbox.x == 10.0
        assert bbox.y == 20.0
        assert bbox.width == 30.0
        assert bbox.height == 40.0

    def test_mlx_backend_parse_bbox_error(self):
        """Test parsing error response."""
        backend = MLXBackend()
        response = '{"error": "not found"}'
        bbox = backend._parse_bbox_response(response, 100, 100)
        assert bbox is None

    def test_mlx_backend_parse_bbox_invalid_json(self):
        """Test parsing invalid JSON."""
        backend = MLXBackend()
        response = "not valid json"
        bbox = backend._parse_bbox_response(response, 100, 100)
        assert bbox is None

    def test_mlx_backend_parse_bbox_with_text(self):
        """Test parsing bbox with surrounding text."""
        backend = MLXBackend()
        response = 'The element is at {"x": 25, "y": 35, "width": 50, "height": 20} in the image.'
        bbox = backend._parse_bbox_response(response, 200, 200)
        assert bbox is not None
        assert bbox.x == 50.0  # 25% of 200
        assert bbox.y == 70.0  # 35% of 200


class TestAnthropicBackend:
    """Tests for Anthropic backend."""

    def test_anthropic_backend_requires_key(self):
        """Test Anthropic backend requires API key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError, match="API key required"):
                AnthropicBackend()

    def test_anthropic_backend_with_key(self):
        """Test Anthropic backend with API key."""
        backend = AnthropicBackend(api_key="test-key")
        assert backend.api_key == "test-key"
        assert backend.model == "claude-sonnet-4-20250514"

    def test_anthropic_backend_custom_model(self):
        """Test Anthropic backend with custom model."""
        backend = AnthropicBackend(api_key="test-key", model="claude-3-opus-20240229")
        assert backend.model == "claude-3-opus-20240229"


class TestOpenAIBackend:
    """Tests for OpenAI backend."""

    def test_openai_backend_requires_key(self):
        """Test OpenAI backend requires API key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError, match="API key required"):
                OpenAIBackend()

    def test_openai_backend_with_key(self):
        """Test OpenAI backend with API key."""
        backend = OpenAIBackend(api_key="test-key")
        assert backend.api_key == "test-key"
        assert backend.model == "gpt-4o"


class TestGeminiBackend:
    """Tests for Gemini backend."""

    def test_gemini_backend_requires_key(self):
        """Test Gemini backend requires API key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError, match="API key required"):
                GeminiBackend()

    def test_gemini_backend_with_key(self):
        """Test Gemini backend with API key."""
        backend = GeminiBackend(api_key="test-key")
        assert backend.api_key == "test-key"
        assert backend.model == "gemini-2.0-flash"

    def test_gemini_backend_custom_model(self):
        """Test Gemini backend with custom model."""
        backend = GeminiBackend(api_key="test-key", model="gemini-1.5-pro")
        assert backend.model == "gemini-1.5-pro"

    def test_gemini_backend_env_key(self):
        """Test Gemini backend reads key from environment."""
        with patch.dict("os.environ", {"GOOGLE_API_KEY": "env-test-key"}):
            backend = GeminiBackend()
            assert backend.api_key == "env-test-key"

    def test_gemini_backend_parse_bbox_valid(self):
        """Test parsing valid bbox response."""
        backend = GeminiBackend(api_key="test-key")
        response = '{"x": 10, "y": 20, "width": 30, "height": 40}'
        bbox = backend._parse_bbox_response(response, 100, 100)
        assert bbox is not None
        assert bbox.x == 10.0
        assert bbox.y == 20.0
        assert bbox.width == 30.0
        assert bbox.height == 40.0

    def test_gemini_backend_parse_bbox_error(self):
        """Test parsing error response."""
        backend = GeminiBackend(api_key="test-key")
        response = '{"error": "not found"}'
        bbox = backend._parse_bbox_response(response, 100, 100)
        assert bbox is None


class TestVLMDetector:
    """Tests for VLMDetector class."""

    def test_detector_creation(self):
        """Test creating detector with mock backend."""
        mock_backend = MagicMock(spec=VLMBackend)
        detector = VLMDetector(backend=mock_backend)
        assert detector.backend == mock_backend

    def test_detector_detect_found(self):
        """Test detector returns coordinates when element found."""
        mock_backend = MagicMock(spec=VLMBackend)
        mock_backend.detect_element.return_value = BoundingBox(x=10, y=20, width=100, height=50)

        detector = VLMDetector(backend=mock_backend)
        result = detector.detect(b"image_data", "Save button", 800, 600)

        assert result == (60.0, 45.0)  # center of bbox
        mock_backend.detect_element.assert_called_once_with(b"image_data", "Save button", 800, 600)

    def test_detector_detect_not_found(self):
        """Test detector returns None when element not found."""
        mock_backend = MagicMock(spec=VLMBackend)
        mock_backend.detect_element.return_value = None

        detector = VLMDetector(backend=mock_backend)
        result = detector.detect(b"image_data", "Nonexistent button", 800, 600)

        assert result is None


class TestConfigureVLM:
    """Tests for configure_vlm function."""

    def test_configure_vlm_invalid_backend(self):
        """Test configure_vlm with invalid backend."""
        with pytest.raises(ValueError, match="Unknown backend"):
            configure_vlm(backend="invalid")

    def test_configure_vlm_mlx(self):
        """Test configure_vlm with MLX backend."""
        # This won't load the model, just configure
        configure_vlm(backend="mlx")
        detector = get_vlm_detector()
        assert detector is not None
        assert isinstance(detector.backend, MLXBackend)

    def test_configure_vlm_anthropic_requires_key(self):
        """Test configure_vlm with Anthropic requires key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError):
                configure_vlm(backend="anthropic")

    def test_configure_vlm_openai_requires_key(self):
        """Test configure_vlm with OpenAI requires key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError):
                configure_vlm(backend="openai")

    def test_configure_vlm_gemini_requires_key(self):
        """Test configure_vlm with Gemini requires key."""
        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError):
                configure_vlm(backend="gemini")

    def test_configure_vlm_gemini_with_key(self):
        """Test configure_vlm with Gemini backend."""
        configure_vlm(backend="gemini", api_key="test-key")
        detector = get_vlm_detector()
        assert detector is not None
        assert isinstance(detector.backend, GeminiBackend)


class TestDetectElementVisual:
    """Tests for detect_element_visual function."""

    def test_detect_element_visual_no_backends(self):
        """Test detect_element_visual when no backends available."""
        import axterminator.vlm as vlm_module

        # Clear global detector
        vlm_module._vlm_detector = None

        with patch.dict("os.environ", {}, clear=True):
            with patch.object(vlm_module, "configure_vlm", side_effect=ImportError("mlx not installed")):
                result = detect_element_visual(b"image", "button", 100, 100)
                # Should return None when no backend available
                assert result is None

    def test_detect_element_visual_with_detector(self):
        """Test detect_element_visual with configured detector."""
        import axterminator.vlm as vlm_module

        mock_backend = MagicMock(spec=VLMBackend)
        mock_backend.detect_element.return_value = BoundingBox(x=50, y=50, width=20, height=10)
        vlm_module._vlm_detector = VLMDetector(backend=mock_backend)

        result = detect_element_visual(b"image", "button", 100, 100)
        assert result == (60.0, 55.0)  # center


class TestParseResponse:
    """Additional tests for response parsing edge cases."""

    def test_parse_nested_json(self):
        """Test parsing nested JSON responses."""
        backend = MLXBackend()
        response = '{"result": {"x": 10, "y": 20, "width": 30, "height": 40}}'
        # Should find the inner JSON
        bbox = backend._parse_bbox_response(response, 100, 100)
        # First valid JSON object found
        assert bbox is None  # {"result": ...} doesn't have x,y directly

    def test_parse_multiple_json(self):
        """Test parsing response with multiple JSON objects."""
        backend = MLXBackend()
        response = '{"error": "first"} {"x": 10, "y": 20, "width": 30, "height": 40}'
        bbox = backend._parse_bbox_response(response, 100, 100)
        # Should find first complete JSON
        assert bbox is None  # First one has error

    def test_percentage_conversion(self):
        """Test percentage to pixel conversion."""
        backend = MLXBackend()
        # 50% of 1920x1080
        response = '{"x": 50, "y": 50, "width": 10, "height": 10}'
        bbox = backend._parse_bbox_response(response, 1920, 1080)
        assert bbox.x == 960.0  # 50% of 1920
        assert bbox.y == 540.0  # 50% of 1080
        assert bbox.width == 192.0  # 10% of 1920
        assert bbox.height == 108.0  # 10% of 1080
