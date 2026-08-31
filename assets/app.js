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
