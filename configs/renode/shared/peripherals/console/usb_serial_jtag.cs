//
// ESP32-C6 USB Serial JTAG — Renode peripheral model
//
// Minimal register model for the esp-hal 1.x blocking UsbSerialJtag driver.
// The driver polls EP1_CONF.EP1_WR_RDY (bit 1) before every 64-byte chunk,
// then writes bytes one-at-a-time to EP1, then sets WR_DONE (bit 0).
// NS16550 returns 0 for its IER register at the same offset, causing the
// firmware to spin forever. This model fixes that.
//
// Implemented registers (ESP32-C6 TRM v1.2, §USB Serial/JTAG Controller):
//   0x000  EP1       bits[7:0] = byte pushed to IN endpoint FIFO
//   0x004  EP1_CONF  bit 0 = WR_DONE (write-only trigger); bit 1 = EP1_WR_RDY (read-only, always 1)
//
// All other offsets return 0 and silently discard writes.
// Extends UARTBase → IUART, so showAnalyzer and connector Connect work.
//
using Antmicro.Renode.Peripherals.UART;
using Antmicro.Renode.Peripherals.Bus;
using Antmicro.Renode.Core;

namespace Antmicro.Renode.Peripherals.UART
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class ESP32C6_UsbSerialJtag : UARTBase, IDoubleWordPeripheral, IKnownSize
    {
        public ESP32C6_UsbSerialJtag(IMachine machine) : base(machine) { }

        public long Size => 0x1000;

        public uint ReadDoubleWord(long offset)
        {
            if (offset == EP1_CONF_OFFSET)
                return EP1_WR_RDY_BIT;
            if (offset == INT_RAW_OFFSET)
                return SOF_INT_BIT;
            return 0u;
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            if (offset == EP1_OFFSET)
                TransmitCharacter((byte)(value & 0xFFu));
            // EP1_CONF WR_DONE: hardware flush trigger — no-op in simulation.
        }

        public override void Reset() { }
        public override Bits StopBits    => Bits.One;
        public override Parity ParityBit => Parity.None;
        public override uint BaudRate    => 0;

        protected override void CharWritten() { }
        protected override void QueueEmptied() { }

        private const long EP1_OFFSET      = 0x000;
        private const long EP1_CONF_OFFSET = 0x004;
        private const long INT_RAW_OFFSET  = 0x008;
        private const uint EP1_WR_RDY_BIT  = 0x2u;
        private const uint SOF_INT_BIT     = 0x2u;
    }
}
