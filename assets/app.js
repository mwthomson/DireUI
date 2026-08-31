document.querySelectorAll('[data-edit-name]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    var row = btn.closest('li');
    row.querySelector('[data-name-display]').hidden = true;
    row.querySelector('[data-name-edit]').hidden = false;
    btn.hidden = true;
  });
});

document.querySelectorAll('[data-cancel-rename]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    var row = btn.closest('li');
    row.querySelector('[data-name-display]').hidden = false;
    row.querySelector('[data-name-edit]').hidden = true;
    row.querySelector('[data-edit-name]').hidden = false;
  });
});

function openDeleteDialog(path, name) {
  document.getElementById('delete-dialog-name').textContent = name;
  document.getElementById('delete-dialog-remove-path').value = path;
  document.getElementById('delete-dialog-delete-path').value = path;
  showDeleteStep(1);
  document.getElementById('delete-dialog').showModal();
}

function showDeleteStep(n) {
  document.getElementById('delete-step-1').hidden = n !== 1;
  document.getElementById('delete-step-2').hidden = n !== 2;
}

document.querySelectorAll('[data-delete-path]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    openDeleteDialog(btn.dataset.deletePath, btn.dataset.deleteName);
  });
});

var showStep2Button = document.getElementById('delete-dialog-show-step-2');
if (showStep2Button) {
  showStep2Button.addEventListener('click', function () {
    showDeleteStep(2);
  });
}

var showStep1Button = document.getElementById('delete-dialog-show-step-1');
if (showStep1Button) {
  showStep1Button.addEventListener('click', function () {
    showDeleteStep(1);
  });
}
