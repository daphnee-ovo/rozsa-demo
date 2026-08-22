    (() => {
      const root = document.documentElement;
      const body = document.body;
      const sceneNames = { empty: 'Quiet start', active: 'Active thread', settings: 'Appearance' };
      const panels = [...document.querySelectorAll('[data-scene-panel]')];
      const sceneControls = [...document.querySelectorAll('[data-scene]')];
      const sceneName = document.getElementById('sceneName');
      const railThemeLabel = document.getElementById('railThemeLabel');
      const themeToggle = document.getElementById('themeToggle');
      const themeIcon = document.getElementById('themeIcon');
      const themeSelect = document.getElementById('themeSelect');
      const textureToggle = document.getElementById('textureToggle');
      const permissionDialog = document.getElementById('permissionDialog');
      const toast = document.getElementById('toast');
      let toastTimer;

      function readStorage(key, fallback) {
        try { return localStorage.getItem(key) || fallback; } catch (error) { return fallback; }
      }

      function writeStorage(key, value) {
        try { localStorage.setItem(key, value); } catch (error) { /* preview storage may be unavailable */ }
      }

      function setTheme(theme, persist = true) {
        const nextTheme = theme === 'dark' ? 'dark' : 'light';
        root.dataset.theme = nextTheme;
        themeSelect.value = nextTheme;
        railThemeLabel.textContent = nextTheme === 'dark' ? 'Dark' : 'Light';
        themeToggle.setAttribute('aria-label', nextTheme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme');
        themeToggle.setAttribute('title', nextTheme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme');
        themeIcon.innerHTML = nextTheme === 'dark'
          ? '<path d="M12 3.8v2.1M12 18.1v2.1M3.8 12h2.1M18.1 12h2.1M6.2 6.2l1.5 1.5M16.3 16.3l1.5 1.5M17.8 6.2l-1.5 1.5M7.7 16.3l-1.5 1.5"/><circle cx="12" cy="12" r="3.8"/>'
          : '<path d="M20 15.2A8.5 8.5 0 0 1 8.8 4 8.5 8.5 0 1 0 20 15.2Z"/>';
        if (persist) writeStorage('rozsa-demo-theme', nextTheme);
      }

      function setScene(scene, persist = true) {
        const nextScene = sceneNames[scene] ? scene : 'empty';
        panels.forEach((panel) => { panel.hidden = panel.dataset.scenePanel !== nextScene; });
        sceneControls.forEach((control) => {
          const isActive = control.dataset.scene === nextScene;
          if (control.classList.contains('scene-tab')) control.setAttribute('aria-selected', String(isActive));
          if (control.classList.contains('rail-button')) control.setAttribute('aria-current', isActive ? 'page' : 'false');
        });
        sceneName.textContent = sceneNames[nextScene];
        if (persist) writeStorage('rozsa-demo-scene', nextScene);
      }

      function showToast(message, tone = '') {
        window.clearTimeout(toastTimer);
        toast.textContent = message;
        toast.classList.toggle('error-toast', tone === 'error');
        toast.hidden = false;
        toastTimer = window.setTimeout(() => { toast.hidden = true; }, 2800);
      }

      function setDialog(open) {
        permissionDialog.hidden = !open;
        permissionDialog.setAttribute('aria-hidden', String(!open));
        if (open) document.getElementById('permissionCancel').focus();
      }

      sceneControls.forEach((control) => control.addEventListener('click', () => setScene(control.dataset.scene)));
      themeToggle.addEventListener('click', () => setTheme(root.dataset.theme === 'dark' ? 'light' : 'dark'));
      themeSelect.addEventListener('change', () => setTheme(themeSelect.value));
      textureToggle.addEventListener('change', () => {
        body.classList.toggle('reduced-texture', !textureToggle.checked);
        writeStorage('rozsa-demo-texture', textureToggle.checked ? 'on' : 'off');
      });

      document.getElementById('thinkingToggle').addEventListener('click', (event) => {
        const button = event.currentTarget;
        const expanded = button.getAttribute('aria-expanded') === 'true';
        button.setAttribute('aria-expanded', String(!expanded));
        document.getElementById('thinkingBody').hidden = expanded;
      });

      document.getElementById('openPermission').addEventListener('click', () => setDialog(true));
      document.getElementById('permissionCancel').addEventListener('click', () => {
        setDialog(false);
        showToast('Permission kept pending.');
      });
      document.getElementById('permissionAllow').addEventListener('click', () => {
        setDialog(false);
        const status = document.querySelector('[data-od-id="thread-notes-panel"] .note-row:nth-child(3)');
        if (status) {
          status.innerHTML = '<i class="status-dot success" aria-hidden="true"></i><p><strong>Approved</strong>Permission granted for this session.</p>';
        }
        showToast('Permission granted for this session.');
      });
      document.querySelector('[data-close-dialog]').addEventListener('click', () => setDialog(false));
      document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape' && !permissionDialog.hidden) setDialog(false);
      });

      document.getElementById('inspectTool').addEventListener('click', () => showToast('Tool details are ready for review.'));
      document.getElementById('attachButton').addEventListener('click', () => showToast('Attachment picker is represented in this visual study.'));
      document.getElementById('modelButton').addEventListener('click', () => showToast('Model selection stays secondary to the thread.'));
      document.getElementById('thinkingButton').addEventListener('click', () => showToast('Thinking is enabled for this thread.'));

      document.getElementById('copyCode').addEventListener('click', async () => {
        const code = document.getElementById('codeContent').innerText;
        try {
          await navigator.clipboard.writeText(code);
          showToast('Release notes copied.');
        } catch (error) {
          showToast('Copy is unavailable in this preview.', 'error');
        }
      });

      document.getElementById('composer').addEventListener('submit', (event) => {
        event.preventDefault();
        const input = document.getElementById('composerInput');
        const value = input.value.trim();
        if (!value) {
          showToast('Write a message before sending.', 'error');
          input.focus();
          return;
        }
        const message = document.createElement('article');
        message.className = 'message message-user';
        message.dataset.odId = `user-message-${Date.now()}`;
        const meta = document.createElement('div');
        meta.className = 'message-meta';
        meta.innerHTML = '<span>You</span><span>now</span>';
        const copy = document.createElement('p');
        const parts = value.match(/^(\s*)(\p{L})([\s\S]*)$/u);
        if (parts) {
          if (parts[1]) copy.append(document.createTextNode(parts[1]));
          const dropCap = document.createElement('span');
          dropCap.className = 'drop-cap';
          dropCap.textContent = parts[2].toUpperCase();
          copy.append(dropCap, document.createTextNode(parts[3]));
        } else {
          copy.textContent = value;
        }
        message.append(meta, copy);
        const threadColumn = document.querySelector('.thread-column');
        threadColumn.insertBefore(message, document.querySelector('.thinking-block'));
        input.value = '';
        showToast('Message added to the thread.');
      });

      document.getElementById('restoreDefaults').addEventListener('click', () => {
        setTheme('light');
        textureToggle.checked = true;
        body.classList.remove('reduced-texture');
        setScene('settings');
        writeStorage('rozsa-demo-texture', 'on');
        showToast('Appearance restored to warm paper.');
      });

      const savedTheme = readStorage('rozsa-demo-theme', 'light');
      const savedScene = readStorage('rozsa-demo-scene', 'empty');
      const savedTexture = readStorage('rozsa-demo-texture', 'on');
      setTheme(savedTheme, false);
      textureToggle.checked = savedTexture !== 'off';
      body.classList.toggle('reduced-texture', savedTexture === 'off');
      setScene(savedScene, false);
    })();

