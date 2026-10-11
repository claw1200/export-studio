# Vendored rbxport USB sync

The USB exporter is the sync system from [rbxport](https://github.com/chrisle/rbxport)
1.2.0 (`a029c0c2b9d6c9732b4634ee8be42ff66bbfc56c`), written by Chris Le.

It is GPL-2.0-or-later. See `LICENSE` and the full license text in `COPYING`.
`es-usb` links these crates, so that program is a combined work under the same license.

`es-usb` is the command the Python engine runs. It reads one JSON job and
calls `rbl_export::export_cancellable`.
