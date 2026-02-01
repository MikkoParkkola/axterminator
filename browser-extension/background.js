// Background service worker for axterminator recorder

// Initialize storage on install
chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.set({
    isRecording: false,
    actions: []
  });
});

// Relay messages between popup and content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'actionRecorded') {
    // Forward to popup if open
    chrome.runtime.sendMessage(message).catch(() => {
      // Popup not open, store action
      chrome.storage.local.get(['actions'], (result) => {
        const actions = result.actions || [];
        actions.push(message.action);
        chrome.storage.local.set({ actions });
      });
    });
  }
});

// Keep service worker alive during recording
chrome.storage.onChanged.addListener((changes, namespace) => {
  if (changes.isRecording) {
    console.log('Recording state changed:', changes.isRecording.newValue);
  }
});
