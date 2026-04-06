# Geschwister Schneider USB CAN

# udev Rules

```no_build
# Geschwister Schneider
SUBSYSTEM=="usb", ATTR{idVendor}=="1d50", ATTR{idProduct}=="606f", MODE="0666"
# CANdleLight
SUBSYSTEM=="usb", ATTR{idVendor}=="1209", ATTR{idProduct}=="2323", MODE="0666"
# CES CANext FD
SUBSYSTEM=="usb", ATTR{idVendor}=="1cd2", ATTR{idProduct}=="606f", MODE="0666"
# ABE CAN Debugger FD
SUBSYSTEM=="usb", ATTR{idVendor}=="1cd2", ATTR{idProduct}=="16d0", MODE="0666"
# Xylanta Saint3
SUBSYSTEM=="usb", ATTR{idVendor}=="16d0", ATTR{idProduct}=="0f30", MODE="0666"
# CANnectivity
SUBSYSTEM=="usb", ATTR{idVendor}=="1209", ATTR{idProduct}=="ca01", MODE="0666"
```

```sh
sudo udevadm control --reload-rules
sudo udevadm trigger
```
