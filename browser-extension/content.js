// Content script for axterminator recorder
// Captures user interactions and extracts element attributes

let isRecording = false;

// Listen for toggle from popup
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'toggleRecording') {
    isRecording = message.isRecording;

    if (isRecording) {
      startRecording();
    } else {
      stopRecording();
    }
  }
});

// Check initial state
chrome.storage.local.get(['isRecording'], (result) => {
  isRecording = result.isRecording || false;
  if (isRecording) {
    startRecording();
  }
});

function startRecording() {
  document.addEventListener('click', handleClick, true);
  document.addEventListener('input', handleInput, true);
  document.addEventListener('change', handleChange, true);

  // Visual indicator
  showRecordingIndicator();
}

function stopRecording() {
  document.removeEventListener('click', handleClick, true);
  document.removeEventListener('input', handleInput, true);
  document.removeEventListener('change', handleChange, true);

  hideRecordingIndicator();
}

function handleClick(event) {
  if (!isRecording) return;

  const element = event.target;
  const action = extractElementInfo(element);
  action.type = 'click';

  sendAction(action);
}

function handleInput(event) {
  // Debounce input events
  clearTimeout(event.target._inputTimeout);
  event.target._inputTimeout = setTimeout(() => {
    if (!isRecording) return;

    const element = event.target;
    const action = extractElementInfo(element);
    action.type = 'input';
    action.value = element.value;

    sendAction(action);
  }, 500);
}

function handleChange(event) {
  if (!isRecording) return;

  const element = event.target;
  const action = extractElementInfo(element);
  action.type = 'change';
  action.value = element.value;

  sendAction(action);
}

function extractElementInfo(element) {
  return {
    tag: element.tagName.toLowerCase(),
    id: element.id || null,
    className: element.className || null,
    text: getElementText(element),
    testId: element.getAttribute('data-testid') ||
            element.getAttribute('data-test-id') ||
            element.getAttribute('data-cy') ||
            null,
    ariaLabel: element.getAttribute('aria-label') || null,
    ariaRole: element.getAttribute('role') || null,
    placeholder: element.getAttribute('placeholder') || null,
    name: element.getAttribute('name') || null,
    type: element.getAttribute('type') || null,
    href: element.getAttribute('href') || null,
    xpath: getXPath(element),
    rect: element.getBoundingClientRect(),
    timestamp: Date.now()
  };
}

function getElementText(element) {
  // Get visible text, truncated
  const text = element.innerText || element.textContent || '';
  return text.trim().substring(0, 100);
}

function getXPath(element) {
  if (element.id) {
    return `//*[@id="${element.id}"]`;
  }

  const parts = [];
  let current = element;

  while (current && current.nodeType === Node.ELEMENT_NODE) {
    let index = 1;
    let sibling = current.previousSibling;

    while (sibling) {
      if (sibling.nodeType === Node.ELEMENT_NODE &&
          sibling.tagName === current.tagName) {
        index++;
      }
      sibling = sibling.previousSibling;
    }

    const tagName = current.tagName.toLowerCase();
    const part = index > 1 ? `${tagName}[${index}]` : tagName;
    parts.unshift(part);

    current = current.parentNode;
  }

  return '/' + parts.join('/');
}

function sendAction(action) {
  chrome.runtime.sendMessage({
    type: 'actionRecorded',
    action: action
  });
}

// Recording indicator
let indicator = null;

function showRecordingIndicator() {
  if (indicator) return;

  indicator = document.createElement('div');
  indicator.id = 'axterminator-recording-indicator';
  indicator.innerHTML = `
    <style>
      #axterminator-recording-indicator {
        position: fixed;
        top: 10px;
        right: 10px;
        background: rgba(255, 71, 87, 0.95);
        color: white;
        padding: 8px 16px;
        border-radius: 20px;
        font-family: -apple-system, sans-serif;
        font-size: 13px;
        font-weight: 600;
        z-index: 999999;
        display: flex;
        align-items: center;
        gap: 8px;
        box-shadow: 0 2px 10px rgba(0,0,0,0.3);
      }
      #axterminator-recording-indicator::before {
        content: "";
        width: 10px;
        height: 10px;
        background: white;
        border-radius: 50%;
        animation: axterminator-blink 1s infinite;
      }
      @keyframes axterminator-blink {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.3; }
      }
    </style>
    Recording...
  `;
  document.body.appendChild(indicator);
}

function hideRecordingIndicator() {
  if (indicator) {
    indicator.remove();
    indicator = null;
  }
}
