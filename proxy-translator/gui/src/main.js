// Translatore Desktop UI Controller

document.addEventListener('DOMContentLoaded', () => {
  initTabNavigation();
  initMasterToggle();
  initRulePresets();
  initLogConsole();
  fetchStatus();
});

// Tab Switching Logic
function initTabNavigation() {
  const navButtons = document.querySelectorAll('.nav-item');
  const tabContents = document.querySelectorAll('.tab-content');

  navButtons.forEach(button => {
    button.addEventListener('click', () => {
      const targetTab = button.dataset.tab;

      navButtons.forEach(btn => btn.classList.remove('active'));
      tabContents.forEach(tab => tab.classList.remove('active'));

      button.classList.add('active');
      document.getElementById(`tab-${targetTab}`).classList.add('active');
    });
  });
}

// Master Toggle Handler
function initMasterToggle() {
  const toggle = document.getElementById('master-proxy-toggle');
  const statusText = document.getElementById('master-status-text');

  toggle.addEventListener('change', (e) => {
    if (e.target.checked) {
      statusText.textContent = 'CONNECTED';
      statusText.style.color = 'var(--accent-green)';
      appendLogLine('[HTTP/SOCKS5] Agent proxies ENABLED', 'ok');
    } else {
      statusText.textContent = 'DISCONNECTED';
      statusText.style.color = 'var(--accent-red)';
      appendLogLine('[HTTP/SOCKS5] Agent proxies PAUSED', 'info');
    }
  });
}

// Rule Presets Bar
function initRulePresets() {
  const presetButtons = document.querySelectorAll('.btn-preset');
  const rulesTableBody = document.getElementById('rules-table-body');

  presetButtons.forEach(btn => {
    btn.addEventListener('click', () => {
      presetButtons.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');

      if (btn.id === 'preset-bounty') {
        rulesTableBody.innerHTML = `
          <tr><td><code>*.target.com</code></td><td><span class="tag tag-http">HTTP</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
          <tr><td><code>api.github.com</code></td><td><span class="tag tag-http">HTTP</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
          <tr><td><code>192.168.1.*:22</code></td><td><span class="tag tag-tcp">TCP</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
        `;
      } else if (btn.id === 'preset-strict') {
        rulesTableBody.innerHTML = `
          <tr><td><code>ifconfig.me</code></td><td><span class="tag tag-http">HTTP</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
          <tr><td><code>httpbin.org</code></td><td><span class="tag tag-http">HTTP</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
        `;
      } else if (btn.id === 'preset-dev') {
        rulesTableBody.innerHTML = `
          <tr><td><code>*</code></td><td><span class="tag tag-http">ALL</span></td><td><span class="status-pill active">Active</span></td><td><button class="btn-danger-sm">Delete</button></td></tr>
        `;
      }
    });
  });
}

// Log Console Handler
function initLogConsole() {
  const clearBtn = document.getElementById('btn-clear-logs');
  const consoleEl = document.getElementById('log-console');

  if (clearBtn && consoleEl) {
    clearBtn.addEventListener('click', () => {
      consoleEl.innerHTML = '<div class="log-line info">--- Logs cleared ---</div>';
    });
  }
}

function appendLogLine(message, type = 'info') {
  const consoleEl = document.getElementById('log-console');
  if (!consoleEl) return;

  const timestamp = new Date().toLocaleTimeString();
  const line = document.createElement('div');
  line.className = `log-line ${type}`;
  line.textContent = `[${timestamp}] ${message}`;
  consoleEl.appendChild(line);
  consoleEl.scrollTop = consoleEl.scrollHeight;
}

// Fetch status mock/real
async function fetchStatus() {
  try {
    const res = await fetch('http://127.0.0.1:8888/httpbin.org/ip', { method: 'GET' }).catch(() => null);
    if (res) {
      appendLogLine('Proxy check successful - Exit IP confirmed', 'ok');
    }
  } catch (e) {
    // Ignore offline state in development mode
  }
}
