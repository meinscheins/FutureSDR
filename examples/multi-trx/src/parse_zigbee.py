#!/usr/bin/env python3

import socket
from scapy.layers.dot15d4 import *
from scapy.packet import Packet
from scapy.all import *

UDP_IP = "127.0.0.1"
UDP_PORT = 55557
conf.dot15d4_protocol = 'zigbee'

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind((UDP_IP, UDP_PORT))


while True:
    data, addr = sock.recvfrom(1024 * 2)
    packet = Dot15d4(data)
    print(packet.summary())



