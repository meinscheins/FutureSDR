from typing import Tuple, List, Dict, Optional

import requests.exceptions
from PyQt5 import QtWidgets, uic
import sys
import socket
from functools import partial
import time
from matplotlib.backends.qt_compat import QtCore, QtWidgets
# from PyQt5 import QtWidgets, QtCore
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg as FigureCanvas
import matplotlib.pyplot as plt
import numpy as np
import struct
import cmath
import signal

signal.signal(signal.SIGINT, signal.SIG_DFL)

# worker.py
from PyQt5.QtCore import QThread, QObject, pyqtSignal, pyqtSlot
import time

from PhyController import PhyController


class UDPReceiverWorker(QObject):
    """
    https://stackoverflow.com/a/33453124
    """
    finished = pyqtSignal()
    dataReady = pyqtSignal(bytes)

    def __init__(self, rx_port: int):
        super().__init__()
        self.rx_port = rx_port

    @pyqtSlot()
    def receive_packet(self):  # A slot takes no params
        metrics_receive_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.getprotobyname("udp"))
        metrics_receive_socket.bind(('', self.rx_port))
        while True:
            message, address = metrics_receive_socket.recvfrom(1024)
            self.dataReady.emit(message)


SDR_ENDPOINTS = (
    ("10.193.0.73", 1339),
    ("10.193.0.75", 1339)
)

PHY_WIFI = 0
PHY_ZIGBEE = 1

CHANEM_ENDPOINT = ("10.193.0.73", 1341)

RX_PORT_PACKAGE_COUNTER = 1340
RX_PORT_POSITION = 1342

COUNTING_BINS = [
    ('server', 'tx'),
    ('server', 'rx'),
    ('client', 'tx'),
    ('client', 'rx')
]

MAX_PATH_LOSS_FOR_PLOTTING = 120
PLOTTING_INTERVAL_MS = 1000
RATE_SMOOTHING_FACTOR = 3
"""
averages rate over the last x intervals for smoother plotting
"""

SPEED_OF_LIGHT: float = 299_792_458.
LAMBDA: float = SPEED_OF_LIGHT / 2.45e9
EPSILON_R: float = 1.02
DELAY_PER_TAP = (41 - 1) / (2.0 * 20e6)  # seconds

STATION_X: float = 0.0
STATION_Y: float = 0.0
STATION_Z: float = 1.5  # TODO 1.5


class MyFigureCanvas(FigureCanvas):
    """
    This is the FigureCanvas in which the live plot is drawn.
    https://stackoverflow.com/q/57891219
    """

    def __init__(
            self, x_len: int, interval: int, data_getter_callback: callable,
            y_range: Optional[list], y_label: Optional[str] = None, small: Optional[bool] = False,
            y_scale: str = 'linear'
    ) -> None:
        """
        :param x_len:       The nr of data points shown in one plot.
        :param y_range:     Range on y-axis.
        :param interval:    Get a new datapoint every .. milliseconds.

        """
        super().__init__(plt.Figure(tight_layout=True))
        # Range settings
        self._x_len_ = x_len
        self._y_range_ = y_range
        self.data_getter_callback = data_getter_callback

        self.prev_val = 0

        # Store two lists _x_ and _y_
        self._x_ = list(range(0, x_len))
        self._y_segments = [[0] * x_len]
        self._colours = [0]
        self.colormap = plt.cm.Dark2.colors

        # Store a figure ax
        self._ax_ = self.figure.subplots()
        if not small:
            self._ax_.set_xticks([0, 29, 59, 89, 119], ["-2min", "-1.5min", "-1min", "-30sek", "0"])
            # self._ax_.set_xlabel("Time")
            if y_label is not None:
                self._ax_.set_ylabel(y_label)
        if y_range is not None:
            y_range_size = self._y_range_[1] - self._y_range_[0]
            self._ax_.set_ylim(
                ymin=self._y_range_[0] - 0.05 * y_range_size,
                ymax=self._y_range_[1] + 0.05 * y_range_size
            )
        self._ax_.set_yscale(y_scale)
        self._lines_ = []
        x_counter = 0
        for y_segment, colour in zip(self._y_segments, self._colours):
            x_counter_new = x_counter + len(y_segment)
            self._lines_ += self._ax_.plot(
                list(range(x_counter, x_counter_new)),
                y_segment,
                "-", c=self.colormap[colour]
            )
        self.draw()  # added

        # Initiate the timer
        self._timer_ = self.new_timer(interval, [(self._update_canvas_, (), {})])
        self._timer_.start()
        return

    def _update_canvas_(self) -> None:
        '''
        This function gets called regularly by the timer.

        '''
        try:
            new_val = self.data_getter_callback()
            self.prev_val = new_val
        except ArithmeticError:
            new_val = self.prev_val
        if isinstance(new_val, tuple):
            val, colour = new_val
        else:
            val = new_val
            colour = 0
        if colour == self._colours[-1]:
            # extend last line segment
            self._y_segments[-1].append(round(val, 4))  # Add new datapoint
        else:
            # add new line segment with different colour
            self._y_segments.append([
                self._y_segments[-1][-1],  # replicate previous datapoint to avoid gaps in the graph
                round(val, 4)]
            )  # Add new datapoint
            self._colours.append(colour)
            # create Line2D object for new line segment
            y_data = self._y_segments[-1]
            self._lines_ += self._ax_.plot(
                list(range(self._x_len_ - len(y_data), self._x_len_)),
                y_data,
                "-", c=self.colormap[self._colours[-1]]
            )
        if len(self._y_segments[0]) > 1:
            # shorten first line segment
            self._y_segments[0] = self._y_segments[0][1:]
        else:
            # discard first line segment as it has been moved out of the visible scope
            self._y_segments = self._y_segments[1:]
            self._y_segments[0] = self._y_segments[0][1:]  # remove previously replicated datapoint
            self._colours = self._colours[1:]
            self._lines_ = self._lines_[1:]
        for line, y_data in zip(self._lines_, self._y_segments):
            line.set_ydata(y_data)
        x_counter = 0
        for line in self._lines_:
            x_counter_new = x_counter + len(line.get_data()[1])
            x_data_new = list(range(x_counter, x_counter_new))
            line.set_xdata(x_data_new)
            x_counter = x_counter_new - 1  # handle replicated datapoint

        self._ax_.draw_artist(self._ax_.patch)
        for line in self._lines_:
            self._ax_.draw_artist(line)
        self.update()
        self.flush_events()
        return


class MyBarFigureCanvas(FigureCanvas):

    def __init__(
            self, x_len: int, interval: int, data_getter_callback: callable,
            y_range: Optional[list], y_label: Optional[str] = None, x_label: Optional[str] = None,
            y_ticks: Optional[Tuple[List[float], List[str]]] = None,
            x_tick_label: Optional[list[str]] = None, small: Optional[bool] = False,
            y_scale: str = 'linear', basey: Optional[int] = None
    ) -> None:
        """
        :param x_len:       The nr of data points shown in one plot.
        :param y_range:     Range on y-axis.
        :param interval:    Get a new datapoint every .. milliseconds.

        """
        super().__init__(plt.Figure(tight_layout=True))
        # Range settings
        self.data_getter_callback = data_getter_callback

        self.prev_val = 0

        # Store a figure ax
        self._ax_ = self.figure.subplots()
        if not small:
            if y_label is not None:
                self._ax_.set_ylabel(y_label)
            if x_label is not None:
                self._ax_.set_xlabel(x_label)
        if y_range is not None:
            y_range_size = y_range[1] - y_range[0]
            # self.bottom = [-0.05 * y_range_size, ] * x_len
            self._ax_.set_ylim(
                ymin=y_range[0] - 0.05 * y_range_size,
                ymax=y_range[1] + 0.05 * y_range_size
            )
        # else:
        self.bottom = [0, ] * x_len
        if y_ticks is not None:
            self._ax_.set_yticks(y_ticks[0], y_ticks[1])
        self._ax_.set_yscale(y_scale)

        # Store two lists _x_ and _y_
        self._x_ = list(range(x_len))
        self.bar_lengths = [-b for b in self.bottom]

        self.line = self._ax_.axhline(y=0, color="black")
        self.bars = self._ax_.bar(
            x=self._x_,
            height=self.bar_lengths,
            width=1,
            bottom=self.bottom,
            tick_label=x_tick_label,
            align='center'
        )
        self.draw()
        # Initiate the timer
        self._timer_ = self.new_timer(interval, [(self._update_canvas_, (), {})])
        self._timer_.start()
        return

    def _update_canvas_(self) -> None:
        '''
        This function gets called regularly by the timer.

        '''
        new_heights = self.data_getter_callback()
        redraw = []
        for i in self._x_:
            # if self.bars[i].get_height() + self.bottom[i] != new_heights[i]:
            if True:
                self.bars[i].set_height(new_heights[i])
                # self.bars[i].set_height(new_heights[i] - self.bottom[i])
                redraw.append(self.bars[i])
        if len(redraw) > 0:
            self._ax_.draw_artist(self._ax_.patch)
            self._ax_.draw_artist(self.line)
        for bar in redraw:
            self._ax_.draw_artist(bar)
        self.update()
        self.flush_events()
        return


# def  calculate_pl_two_ray(pos: np.ndarray) -> float :
#     d = np.linalg.norm(pos[:2])
#     if d == 0.:
#         return 0.
#     d_los = np.linalg.norm(pos)
#     d_ref = np.linalg.norm(pos * np.array((1, 1, 2)))
#     cos_theta = d / d_ref
#     sin_theta = (z + STATION_Z) / d_ref
#     gamma = ((EPSILON_R - cos_theta.powi(2)) as f64).sqrt() as f32
#     gamma = (sin_theta - gamma) / (sin_theta + gamma)
#     phi = 2. * PI * ((d_ref - d_los) / LAMBDA)
#     i_phi = Complex::new(0., 1.) * phi
#     e_raised_i_phi = i_phi.exp()
#     pl = 20. * (4. * np.pi * (d / LAMBDA) * ( 1. / (1. + gamma * e_raised_i_phi).norm())).log10()
#     return pl


class Ui(QtWidgets.QMainWindow):

    def __init__(self, call_arg: Optional):
        super(Ui, self).__init__()  # Call the inherited classes __init__ method

        uic.loadUi('gui.ui', self)  # Load the .ui file

        if call_arg is None:
            self.tabWidget_2.removeTab(5)
        else:
            self.magic_scaling_factor_send.clicked.connect(self.send_magic_scaling_factor)

        self.get_datapoint_rate_ag = partial(self.get_datapoint_rate, keys=(('server', 'rx'), ('client', 'tx')))
        self.get_datapoint_rate_ga = partial(self.get_datapoint_rate, keys=(('client', 'rx'), ('server', 'tx')))
        self.get_datapoint_rate_combined = lambda: (
            (
                    self.get_datapoint_rate(keys=(('server', 'rx'), ('client', 'tx')))[0]
                    +
                    self.get_datapoint_rate(keys=(('client', 'rx'), ('server', 'tx')))[0]
            ) / 2,
            0 if self.radio_button_wifi.isChecked() else 1
        )

        self.workers = [
            self.init_receiver(RX_PORT_PACKAGE_COUNTER, self.on_data_ready_package_counter),
            self.init_receiver(RX_PORT_POSITION, self.on_data_ready_position),
        ]

        # delivery rate tab
        self.plot_delivery_rate_ga = MyFigureCanvas(
            x_len=120, y_range=[0, 1], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_rate_ga,
            y_label="Delivery Rate"
        )
        self.layout_canvas_delivery_rate_ag.addWidget(self.plot_delivery_rate_ga)
        self.plot_delivery_rate_ag = MyFigureCanvas(
            x_len=120, y_range=[0, 1], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_rate_ag,
            y_label="Delivery Rate"
        )
        self.layout_canvas_delivery_rate_ga.addWidget(self.plot_delivery_rate_ag)
        self.plot_delivery_rate_combined_small = MyFigureCanvas(
            x_len=60, y_range=[0, 1], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_rate_combined,
            small=True,
            y_label="Delivery Rate"
        )
        self.canvas_small_delivery_rate.addWidget(self.plot_delivery_rate_combined_small)
        # path loss tab
        self.plot_path_loss_freespace = MyFigureCanvas(
            x_len=120, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_pl_fs,
            y_label="Path Loss (dB)"
        )
        self.plot_path_loss_freespace_small = MyFigureCanvas(
            x_len=60, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_pl_fs,
            small=True,
            y_label="Path Loss (dB)"
        )
        self.layout_canvas_path_loss_fs.addWidget(self.plot_path_loss_freespace)
        self.plot_path_loss_fe2r = MyFigureCanvas(
            x_len=120, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_pl_fe2r,
            y_label="Path Loss (dB)"
        )
        self.plot_path_loss_fe2r_small = MyFigureCanvas(
            x_len=60, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_pl_fe2r,
            small=True,
            y_label="Path Loss (dB)"
        )
        self.layout_canvas_path_loss_fe2r.addWidget(self.plot_path_loss_fe2r)
        self.plot_path_loss_2r_dir = MyFigureCanvas(
            x_len=120, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=lambda: -self.manual_path_loss_value,
            y_label="Path Loss (dB)"
        )
        self.plot_path_loss_2r_dir_small = MyFigureCanvas(
            x_len=60, y_range=[-MAX_PATH_LOSS_FOR_PLOTTING, 0], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=lambda: -self.manual_path_loss_value,
            small=True,
            y_label="Path Loss (dB)"
        )
        self.layout_canvas_path_loss_2r_dir.addWidget(self.plot_path_loss_2r_dir)
        self.canvas_small_pl_1.addWidget(self.plot_path_loss_freespace_small)
        self.canvas_small_pl_2.addWidget(self.plot_path_loss_fe2r_small)
        self.canvas_small_pl_3.addWidget(self.plot_path_loss_2r_dir_small)
        # position tab
        self.plot_position_distance = MyFigureCanvas(
            x_len=120, y_range=[0, 750], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_distance,
            y_label="Distance to Basestation (m)"
        )
        self.layout_canvas_distance.addWidget(self.plot_position_distance)
        self.plot_position_distance_small = MyFigureCanvas(
            x_len=60, y_range=[0, 750], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=self.get_datapoint_distance,
            small=True,
            y_label="Distance to Basestation (m)"
        )
        self.canvas_small_distance.addWidget(self.plot_position_distance_small)
        self.plot_position_height = MyFigureCanvas(
            x_len=120, y_range=[0, 100], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=lambda: self.uav_pos[2],
            y_label="Height (m)"
        )
        self.layout_canvas_height.addWidget(self.plot_position_height)

        self.plot_taps_amplitude = MyBarFigureCanvas(
            x_len=41, y_range=[-1, 1], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=lambda: self.get_datapoint_taps_for_plotting()[0],
            y_label="Real Response", x_label="Time (μs)",
            x_tick_label=[str(round((x * DELAY_PER_TAP * 1_000_000), 2)) if x % 10 == 0 else None for x in range(41)],
            y_scale='symlog'
        )
        self.canvas_taps_amplitude.addWidget(self.plot_taps_amplitude)
        self.plot_taps_phase = MyBarFigureCanvas(
            x_len=41, y_range=[-1, 1], interval=PLOTTING_INTERVAL_MS,
            data_getter_callback=lambda: self.get_datapoint_taps_for_plotting()[1],
            # y_ticks=([0, 0.5 * np.pi, np.pi, 1.5 * np.pi, 2 * np.pi], ["", "π/2", "π", "3π/2", "2π"]),
            y_label="Imaginary Response", x_label="Time (μs)",
            x_tick_label=[str(round((x * DELAY_PER_TAP * 1_000_000), 2)) if x % 10 == 0 else None for x in range(41)],
            y_scale='symlog'
        )
        self.canvas_taps_phase.addWidget(self.plot_taps_phase)

        self.pl_reference_plot_fs = FigureCanvas(plt.Figure(tight_layout=True))
        self.pl_reference_plot_fe_2r = FigureCanvas(plt.Figure(tight_layout=True))
        self.pl_reference_plot_man = FigureCanvas(plt.Figure(tight_layout=True))

        self.horizontalSlider.valueChanged.connect(partial(self.init_reference_plots, init=False))

        self.protocol_switching_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.getprotobyname("udp"))
        self.path_loss_switching_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.getprotobyname("udp"))
        self.datapoints = {
            ('server', 'tx'): [],
            ('server', 'rx'): [],
            ('client', 'tx'): [],
            ('client', 'rx'): []
        }
        self.uav_pos: np.ndarray = np.array((0, 0, 0))
        self.uav_orientation: np.ndarray = np.array((0, 0, 0))
        self.taps: tuple[
            int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int,
            int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int,
            int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int,
            int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int,
            int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int,
            int, int
        ] = (
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0
        )
        self.manual_path_loss_value = 50.0

        self.uav_endpoint_controller = None
        self.ground_endpoint_controller = None

        self.init_reference_plots(init=True)
        self.canvas_small_ref_1.addWidget(self.pl_reference_plot_fs)
        self.canvas_small_ref_2.addWidget(self.pl_reference_plot_fe_2r)
        self.canvas_small_ref_3.addWidget(self.pl_reference_plot_man)

        self.restore_settings()
        self.pushButton_2.clicked.connect(self.restore_settings)
        self.pushButton.clicked.connect(self.apply_settings)
        self.pushButton_3.clicked.connect(self.init_endpoint_controllers)

        self.radio_button_wifi.setEnabled(False)
        self.radio_button_zigbee.setEnabled(False)
        self.tabWidget.setEnabled(False)
        self.radio_button_wifi.toggled.connect(partial(self.change_protocol, 0))
        self.radio_button_zigbee.toggled.connect(partial(self.change_protocol, 1))
        self.radio_button_path_loss_freespace.toggled.connect(
            partial(self.select_path_loss_function, new_index=0, invoked_in_gui=True)
        )
        self.radio_button_path_loss_two_ray.toggled.connect(
            partial(self.select_path_loss_function, new_index=1, invoked_in_gui=True)
        )
        self.radio_button_path_loss_manual.toggled.connect(
            partial(self.select_path_loss_function, new_index=2, invoked_in_gui=True)
        )

        # start background workers
        for _, thread in self.workers:
            thread.start()

        self.show()  # Show the GU

    def init_endpoint_controllers(self):
        try:
            self.uav_endpoint_controller = PhyController(
                url="http://10.193.0.73:1337/api/fg/0/",
                center_freq=int(2.45e9), rx_freq_offset=(4_000_000, 4_000_000),
                tx_freq_offset=(4_000_000, 4_000_000),
                rx_gain=(60, 60), tx_gain=(40, 40),
                sample_rate=(4_000_000, 4_000_000),
                rx_device_channel=0,
                tx_device_channel=0,
            )
            self.ground_endpoint_controller = PhyController(
                url="http://10.193.0.75:1337/api/fg/0/",
                center_freq=int(2.45e9), rx_freq_offset=(-4_000_000, -4_000_000),
                tx_freq_offset=(-4_000_000, -4_000_000),
                rx_gain=(60, 60), tx_gain=(40, 40),
                sample_rate=(4_000_000, 4_000_000),
                rx_device_channel=0,
                tx_device_channel=1,
            )
            self.select_path_loss_function(True, 0, invoked_in_gui=False)
            self.stackedWidget.setCurrentIndex(1)
            self.tabWidget.setEnabled(True)
            self.lineEdit_2.setEnabled(False)
            self.lineEdit_7.setEnabled(False)
            self.groupBox_3.setEnabled(True)
            self.radio_button_wifi.setEnabled(True)
            self.radio_button_zigbee.setEnabled(True)
        except requests.exceptions.ConnectionError as e:
            print(e)

    def init_reference_plots(self, init: bool = False):
        x_len = self.horizontalSlider.value()
        self.init_reference_plot(
            self.pl_reference_plot_fs,
            lambda x, y, z: self.path_loss_fs(self.distance(x, y, z)),
            x_len,
            init
        )
        self.init_reference_plot(
            self.pl_reference_plot_fe_2r,
            self.path_loss_fe_2r,
            x_len,
            init
        )
        self.init_reference_plot(
            self.pl_reference_plot_man,
            lambda x, y, z: -self.manual_path_loss_value,
            x_len,
            init
        )

    @staticmethod
    def init_reference_plot(plot: FigureCanvas, data_generation_function: callable, x_len: int, init: bool):
        if init:
            ax = plot.figure.subplots()
        else:
            ax = plot.figure.gca()
        x = range(100)
        x = [x_i / 100 * x_len for x_i in x]
        y = [data_generation_function(0, x_i, 1.5) for x_i in x]
        ax.clear()
        ax.plot(
            x,
            y,
            "-"
        )
        ax.set_ylim(ymin=-MAX_PATH_LOSS_FOR_PLOTTING * 1.05, ymax=MAX_PATH_LOSS_FOR_PLOTTING * 0.05)
        plot.draw()

    @staticmethod
    def init_receiver(port: int, data_ready_callback: callable):
        # 1 - create Worker and Thread inside the Form
        udp_receiver = UDPReceiverWorker(port)  # no parent!
        thread = QThread()  # no parent!
        # 2 - Connect Worker`s Signals to Form method slots to post data.
        udp_receiver.dataReady.connect(data_ready_callback)
        # 3 - Move the Worker object to the Thread object
        udp_receiver.moveToThread(thread)
        # 4 - Connect Worker Signals to the Thread slots
        udp_receiver.finished.connect(thread.quit)
        # 5 - Connect Thread started signal to Worker operational slot method
        thread.started.connect(udp_receiver.receive_packet)
        # 6 - do not start the thread yet, wait after initializations are finished
        return udp_receiver, thread

    def get_datapoint_rate(self, keys: tuple[tuple[str, str], tuple[str, str]]):
        counts = {}
        now = int(time.time_ns() / 1_000_000)
        for key in keys:
            current_data = self.datapoints[key]
            try:
                first_relevant_datapoint = next((
                    i
                    for i, sample
                    in enumerate(current_data)
                    if sample > now - PLOTTING_INTERVAL_MS * RATE_SMOOTHING_FACTOR
                ))
                # discard old samples
                # print(now, current_data[0])
                self.datapoints[key] = self.datapoints[key][first_relevant_datapoint:]
                counts[key] = len(current_data) - first_relevant_datapoint
            except StopIteration:
                counts[key] = 0
        # print(
        #     f"{'AG' if keys[0] == ('server', 'rx') else 'GA'}: "
        #     f"sent {counts[keys[1]]} samples, received {counts[keys[0]]}"
        # )
        if counts[keys[0]] == 0 or counts[keys[1]] == 0:
            val = 0
        else:
            val = counts[keys[0]] / counts[keys[1]]
        val = min(val, 1.0)
        return val, 0 if self.radio_button_wifi.isChecked() else 1

    def get_datapoint_distance(self):
        return np.linalg.norm(self.uav_pos)

    @staticmethod
    def path_loss_fs(d: float) -> float:
        if d == 0:
            return 0
        else:
            return -20. * np.log10(4. * np.pi * (d / LAMBDA))

    def get_datapoint_pl_fs(self):
        d = np.linalg.norm(self.uav_pos)
        return self.path_loss_fs(d)

    @staticmethod
    def distance(x, y, z):
        return np.linalg.norm(np.array((x, y, z)))

    def path_loss_fe_2r(self, x: float, y: float, z: float) -> float:
        d_xy = self.distance(x, y, 0)
        d_los = self.distance(x, y, z)
        if d_los == 0:
            return 0
        d_ref = self.distance(x, y, z + STATION_Z)
        cos_theta = d_xy / d_ref
        sin_theta = (z + STATION_Z) / d_ref
        gamma = np.sqrt(EPSILON_R - cos_theta ** 2)
        gamma = (sin_theta - gamma) / (sin_theta + gamma)
        phi = 2. * np.pi * ((d_ref - d_los) / LAMBDA)
        i_phi = complex(0, 1) * phi
        e_raised_i_phi = cmath.exp(i_phi)
        interference = (1 / abs((1. + gamma * e_raised_i_phi)))
        if interference == np.inf:
            return 0
        # print(interference)  # TODO
        pl = 20. * np.log10(4. * np.pi * (d_los / LAMBDA) * interference)
        return -pl

    def get_datapoint_pl_fe2r(self):
        return self.path_loss_fe_2r(self.uav_pos[0], self.uav_pos[1], self.uav_pos[2])

    def get_datapoint_taps_for_plotting(self):
        taps_complex = [real + 1j * imag for real, imag in zip(self.taps[:41], self.taps[41:])]
        taps_complex = [x / 10000 for x in taps_complex]
        # taps_amplitude = [abs(x) / 1.4 for x in taps_complex]
        # taps_amplitude = [round(x * 4, 4) for x in taps_amplitude]
        # taps_phase = [cmath.phase(x) for x in taps_complex]
        # taps_phase = [
        #     round((p - taps_phase[0]) % (2 * np.pi), 4) if a != 0 else 0
        #     for p, a
        #     in zip(taps_phase, taps_amplitude)
        # ]
        # return taps_amplitude, taps_phase
        taps_real = [round(tap.real, 4) for tap in taps_complex]
        taps_imag = [round(tap.imag, 4) for tap in taps_complex]
        return taps_real, taps_imag

    def on_data_ready_package_counter(self, message):
        endpoint, direction = str(message).strip(" b'").split(',')
        timestamp = int(time.time_ns() / 1_000_000)
        self.datapoints[(endpoint, direction)].append(timestamp)

    def on_data_ready_position(self, message):
        if message[0] == b'P'[0]:
            # received position update
            [x, y, z, r_rad, p_rad, y_rad] = struct.unpack_from('!ffffff', message[1:])
            # print(f"new position update: {[x, y, z, r_rad, p_rad, y_rad]}")
            self.uav_pos = np.array((x, y, z - STATION_Z))  # TODO
            self.uav_orientation = (r_rad, p_rad, y_rad)
        elif message[0] == b'M'[0]:
            # received PL model selection
            new_index, self.manual_path_loss_value = struct.unpack_from('!Bf', message[1:])
            new_index = int(new_index)
            # print(f"new model index: {new_index}")
            self.select_path_loss_function(True, new_index, invoked_in_gui=False)
            self.init_reference_plots()
        elif message[0] == b'T'[0]:
            # received taps
            taps = struct.unpack_from(
                '!hhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh',
                message[1:]
            )
            # print(f"new taps {time.time()}")
            # print(f"new taps: {taps}")
            self.taps = taps
            self.update_taps()
        elif message[0] == b'E'[0]:
            # received control event from gamepad
            # print(f"received control event: {message}")
            event = message[1:3]
            self.parse_control_event(event)
        else:
            print(f"Warning: received invalid message on port 1442 {message}")

    def parse_control_event(self, event: bytes):
        if event == b'TR':  # right trigger
            self.tabWidget_2.setCurrentIndex((self.tabWidget_2.currentIndex() + 1) % self.tabWidget_2.count())
        elif event == b'TL':  # left trigger
            self.tabWidget_2.setCurrentIndex((self.tabWidget_2.currentIndex() - 1) % self.tabWidget_2.count())
        elif event == b'AS':  # green button
            self.radio_button_wifi.setChecked(True)
        elif event == b'AN':  # orange button
            self.radio_button_zigbee.setChecked(True)
        elif event == b'AW':  # blue button
            if self.uav_endpoint_controller is None:
                self.init_endpoint_controllers()
                self.apply_settings()

    def change_protocol(self, new_index: int):
        self.uav_endpoint_controller.select_phy(new_index)
        self.ground_endpoint_controller.select_phy(new_index)

    def update_taps(self):
        pass

    def apply_settings(self):
        malformatted_input = False
        for line_edit in [
            self.lineEdit_5, self.lineEdit_6, self.lineEdit_14, self.lineEdit_13,
            self.lineEdit_17, self.lineEdit_18, self.lineEdit_19, self.lineEdit_20
        ]:
            if not line_edit.text().isdigit():
                line_edit.setStyleSheet("color: red;")
                malformatted_input = True
        for line_edit in [
            self.lineEdit_3, self.lineEdit_4, self.lineEdit_8, self.lineEdit_9
        ]:
            text = line_edit.text()

            if not text.isnumeric() and not (text[0] == '-' and text[1:].isnumeric):
                line_edit.setStyleSheet("color: red;")
                malformatted_input = True
        for line_edit in [
            self.lineEdit, self.lineEdit_10
        ]:
            if not line_edit.text().isnumeric():
                line_edit.setStyleSheet("color: red;")
                malformatted_input = True
        if malformatted_input:
            print("Invalid input. Please check the highlighted settings and try again.")
            return
        for line_edit in [
            self.lineEdit_3, self.lineEdit_4,
            self.lineEdit_8, self.lineEdit_9,
            self.lineEdit_5, self.lineEdit_6, self.lineEdit_14, self.lineEdit_13,
            self.lineEdit_17, self.lineEdit_18, self.lineEdit_19, self.lineEdit_20
        ]:
            line_edit.setStyleSheet("color: black;")
        # WiFi settings
        self.uav_endpoint_controller.set_rx_gain_config(phy=PHY_WIFI, gain=int(self.lineEdit_6.text()))
        self.uav_endpoint_controller.set_tx_gain_config(phy=PHY_WIFI, gain=int(self.lineEdit_5.text()))
        self.uav_endpoint_controller.set_rx_frequency_offset_config(
            phy=PHY_WIFI, offset=int(float(self.lineEdit_4.text()) * 1_000_000)
        )
        self.uav_endpoint_controller.set_tx_frequency_offset_config(
            phy=PHY_WIFI, offset=-int(float(self.lineEdit_3.text()) * 1_000_000)
        )
        self.uav_endpoint_controller.set_sample_rate_config(
            phy=PHY_WIFI,
            sample_rate=int(float(self.lineEdit.text()) * 1_000_000)
        )
        # ZigBee settings
        self.uav_endpoint_controller.set_rx_gain_config(phy=PHY_ZIGBEE, gain=int(self.lineEdit_17.text()))
        self.uav_endpoint_controller.set_tx_gain_config(phy=PHY_ZIGBEE, gain=int(self.lineEdit_18.text()))
        self.uav_endpoint_controller.set_rx_frequency_offset_config(
            phy=PHY_ZIGBEE, offset=int(float(self.lineEdit_9.text()) * 1_000_000)
        )
        self.uav_endpoint_controller.set_tx_frequency_offset_config(
            phy=PHY_ZIGBEE, offset=-int(float(self.lineEdit_8.text()) * 1_000_000)
        )
        self.uav_endpoint_controller.set_sample_rate_config(
            phy=PHY_ZIGBEE,
            sample_rate=int(float(self.lineEdit_10.text()) * 1_000_000)
        )
        self.uav_endpoint_controller.select_phy(self.uav_endpoint_controller.current_phy)
        # WiFi settings
        self.ground_endpoint_controller.set_rx_gain_config(phy=PHY_WIFI, gain=int(self.lineEdit_13.text()))
        self.ground_endpoint_controller.set_tx_gain_config(phy=PHY_WIFI, gain=int(self.lineEdit_14.text()))
        self.ground_endpoint_controller.set_rx_frequency_offset_config(
            phy=PHY_WIFI, offset=-int(float(self.lineEdit_4.text()) * 1_000_000)
        )
        self.ground_endpoint_controller.set_tx_frequency_offset_config(
            phy=PHY_WIFI, offset=int(float(self.lineEdit_3.text()) * 1_000_000)
        )
        self.ground_endpoint_controller.set_sample_rate_config(
            phy=PHY_WIFI,
            sample_rate=int(float(self.lineEdit.text()) * 1_000_000)
        )
        # ZigBee settings
        self.ground_endpoint_controller.set_rx_gain_config(phy=PHY_ZIGBEE, gain=int(self.lineEdit_19.text()))
        self.ground_endpoint_controller.set_tx_gain_config(phy=PHY_ZIGBEE, gain=int(self.lineEdit_20.text()))
        self.ground_endpoint_controller.set_rx_frequency_offset_config(
            phy=PHY_ZIGBEE, offset=-int(float(self.lineEdit_9.text()) * 1_000_000)
        )
        self.ground_endpoint_controller.set_tx_frequency_offset_config(
            phy=PHY_ZIGBEE, offset=int(float(self.lineEdit_8.text()) * 1_000_000)
        )
        self.ground_endpoint_controller.set_sample_rate_config(
            phy=PHY_ZIGBEE,
            sample_rate=int(float(self.lineEdit_10.text()) * 1_000_000)
        )
        self.ground_endpoint_controller.select_phy(self.ground_endpoint_controller.current_phy)

    def restore_settings(self):
        self.lineEdit.setText("4")
        self.lineEdit_6.setText("60")
        self.lineEdit_5.setText("60")
        self.lineEdit_13.setText("60")
        self.lineEdit_14.setText("70")
        self.lineEdit_4.setText("4")
        self.lineEdit_3.setText("-4")
        self.lineEdit_2.setText("2.45")
        self.lineEdit_10.setText("4")
        self.lineEdit_17.setText("60")
        self.lineEdit_18.setText("60")
        self.lineEdit_19.setText("60")
        self.lineEdit_20.setText("70")
        self.lineEdit_9.setText("4")
        self.lineEdit_8.setText("-4")
        self.lineEdit_7.setText("2.45")

    def select_path_loss_function(self, checked: bool, new_index: int, invoked_in_gui: bool):
        if not checked:
            return
        if not invoked_in_gui:
            radio_buttons = (
                self.radio_button_path_loss_freespace,
                self.radio_button_path_loss_two_ray,
                self.radio_button_path_loss_manual
            )
            for button in radio_buttons:
                try:
                    button.toggled.disconnect()
                except TypeError:
                    pass
            radio_buttons[new_index].setChecked(True)
            self.radio_button_path_loss_freespace.toggled.connect(
                partial(self.select_path_loss_function, new_index=0, invoked_in_gui=True)
            )
            self.radio_button_path_loss_two_ray.toggled.connect(
                partial(self.select_path_loss_function, new_index=1, invoked_in_gui=True)
            )
            self.radio_button_path_loss_manual.toggled.connect(
                partial(self.select_path_loss_function, new_index=2, invoked_in_gui=True)
            )
        for i, group_box in enumerate([self.groupBox_14, self.groupBox_13, self.groupBox_16]):
            if i == new_index:
                group_box.setStyleSheet("font-weight: bold;")
            else:
                group_box.setStyleSheet("font-weight: normal;")
        self.stackedWidget_2.setCurrentIndex(new_index)
        self.stackedWidget_3.setCurrentIndex(new_index)
        if invoked_in_gui:
            self.path_loss_switching_socket.sendto(struct.pack("!B", new_index), CHANEM_ENDPOINT)

    def send_magic_scaling_factor(self):
        self.path_loss_switching_socket.sendto(
            struct.pack("!f", float(self.magic_scaling_factor_edit.text())), CHANEM_ENDPOINT
        )


if __name__ == "__main__":
    app = QtWidgets.QApplication(sys.argv)  # Create an instance of QtWidgets.QApplication
    window = Ui(sys.argv[1] if len(sys.argv) > 1 else None)  # Create an instance of our class
    app.exec_()  # Start the application
