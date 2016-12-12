use ::opcodes::{AddressingMode, OpCode};

use cpu::cpu_error::CpuError;
use cpu::flags::StatusFlags;
use cpu::memory_bus::MemoryBus;
use cpu::registers::Registers;
use cpu::stack::Stack;

const DEFAULT_CODE_SEGMENT_START_ADDRESS: u16 = 0xC000;  // Default to a 16KB ROM, leaving 32KB of main memory

const STACK_START: usize = 0x100;
const STACK_END: usize = 0x1FF;

pub enum Operand {
    Immediate(u8),
    Memory(u16),
    Implied,
}

/// A representation of a 6502 microprocessor
pub struct Cpu {
    pub memory: MemoryBus,
    pub registers: Registers,
    pub flags: StatusFlags,
    pub stack: Stack,
}

pub type CpuLoadResult = Result<(), CpuError>;
pub type CpuStepResult = Result<(), CpuError>;

impl Cpu {
    /// Returns a default instance of a Cpu
    pub fn new() -> Cpu {
        Cpu {
            memory: MemoryBus::new(),
            registers: Registers::new(),
            flags: Default::default(),
            stack: Stack::new(),
        }
    }

    /// Loads code into the Cpu main memory at an optional offset. If no
    /// offset is provided, the Cpu will, by default, load the code into
    /// main memory at 0xC000
    pub fn load<T>(&mut self, code: &[u8], addr: T) -> CpuLoadResult
        where T: Into<Option<u16>>
    {
        let addr = addr.into();
        let addr: u16 = if addr.is_some() {
            let addr = addr.unwrap();
            if addr as u32 + code.len() as u32 > u16::max_value() as u32 {
                return Err(CpuError::code_segment_out_of_range(addr));
            } else {
                addr
            }
        } else {
            DEFAULT_CODE_SEGMENT_START_ADDRESS
        };

        for x in 0..code.len() {
            self.memory.write_byte(addr + x as u16, code[x]);
        }

        // Set the Program Counter to point at the
        // start address of the code segment
        self.registers.PC = addr;

        Ok(())
    }

    /// Runs N instructions of code through the Cpu
    pub fn step_n(&mut self, n: u32) -> CpuStepResult {
        for _ in 0..n {
            if self.registers.PC < (self.memory.len() - 1) as u16 {
                self.step()?;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Runs a single instruction of code through the Cpu
    pub fn step(&mut self) -> CpuStepResult {
        let byte = self.memory.read_byte(self.registers.PC);

        if let Some(opcode) = OpCode::from_raw_byte(byte) {
            let operand = self.get_operand_from_opcode(&opcode);

            match opcode.mnemonic {
                "ADC" => self.adc(),
                "AND" => self.and(&operand),
                "ASL" => self.asl(&operand),
                "BCC" => self.bcc(&operand),
                "BCS" => self.bcs(&operand),
                "BEQ" => self.beq(&operand),
                "BIT" => self.bit(&operand),
                "BMI" => self.bmi(&operand),
                "BNE" => self.bne(&operand),
                "BPL" => self.bpl(&operand),
                "BRK" => self.brk(),
                "CLD" => self.set_decimal_flag(false),
                "LDA" => self.lda(&operand),
                "SED" => self.set_decimal_flag(true),
                "STA" => self.sta(&operand),
                _ => return Err(CpuError::unknown_opcode(self.registers.PC, opcode.code)),
            }

            self.registers.PC += opcode.length as u16;

            Ok(())
        } else {
            Err(CpuError::unknown_opcode(self.registers.PC, byte))
        }
    }

    fn get_operand_from_opcode(&self, opcode: &OpCode) -> Operand {
        use ::opcodes::AddressingMode::*;

        let operand_start = self.registers.PC + 1;

        match opcode.mode {
            Unknown => unreachable!(),
            Implied => Operand::Implied,
            Immediate => Operand::Immediate(self.read_byte(operand_start)),
            Relative => Operand::Immediate(self.read_byte(operand_start)),
            Accumulator => Operand::Implied,
            ZeroPage => Operand::Memory((self.read_byte(operand_start) as u16) & 0xFF),
            ZeroPageX => {
                Operand::Memory((self.registers.X as u16 + self.read_byte(operand_start) as u16) &
                                0xFF)
            }
            ZeroPageY => {
                Operand::Memory((self.registers.Y as u16 + self.read_byte(operand_start) as u16) &
                                0xFF)
            }
            Absolute => Operand::Memory(self.read_u16(operand_start)),
            AbsoluteX => Operand::Memory(self.registers.X as u16 + self.read_u16(operand_start)),
            AbsoluteY => Operand::Memory(self.registers.Y as u16 + self.read_u16(operand_start)),
            Indirect => Operand::Memory(self.read_u16(self.read_u16(operand_start))),
            IndirectX => {
                Operand::Memory(self.read_u16((self.registers.X as u16 +
                                               self.read_byte(self.registers.PC + 1) as u16) &
                                              0xFF))
            }
            IndirectY => {
                Operand::Memory(self.registers.Y as u16 +
                                self.read_u16(self.read_byte(self.registers.PC + 1) as u16))
            }
        }
    }

    fn unwrap_immediate(&self, operand: &Operand) -> u8 {
        match *operand {
            Operand::Immediate(byte) => byte,
            Operand::Memory(addr) => self.read_byte(addr),
            Operand::Implied => 0,
        }
    }

    fn unwrap_address(&self, operand: &Operand) -> u16 {
        match *operand {
            Operand::Immediate(byte) => byte as u16,
            Operand::Memory(addr) => addr,
            Operand::Implied => 0,
        }
    }

    // ## OpCode handlers ##

    fn adc(&mut self) {
        // This is implemented on the information provided here:
        // http://www.electrical4u.com/bcd-or-binary-coded-decimal-bcd-conversion-addition-subtraction/
        // and here:
        // http://www.6502.org/tutorials/decimal_mode.html,
        // and here:
        // http://www.atariarchives.org/2bml/chapter_10.php,
        // and also here:
        // http://stackoverflow.com/questions/29193303/6502-emulation-proper-way-to-implement-adc-and-sbc

        let carry = if self.flags.carry { 1 } else { 0 };
        let value = self.read_byte(self.registers.PC + 1) as u16;

        // Do normal binary arithmetic first
        let mut result = self.registers.A as u16 + value as u16 + carry as u16;

        // Handle packed binary coded decimal
        if self.flags.decimal {
            if (self.registers.A as u16 & 0x0F) + (value & 0x0F) + carry > 0x09 {
                result += 0x06;
            }

            if result > 0x99 {
                result += 0x60;
            }

            self.flags.carry = (result & 0x100) == 0x100;
        } else {
            self.flags.carry = result > 0xFF;
        }

        self.flags.zero = result as u8 & 0xFF == 0x00;
        self.flags.sign = result & 0x80 == 0x80;
        self.flags.overflow = ((self.registers.A as u16 ^ result) & (value ^ result) & 0x80) ==
                              0x80;

        self.registers.A = result as u8 & 0xFF;
    }

    fn and(&mut self, operand: &Operand) {
        let value = self.unwrap_immediate(&operand);
        let result = self.registers.A & value;

        self.registers.A = result;

        self.flags.zero = result as u8 & 0xFF == 0;
        self.flags.sign = result & 0x80 == 0x80;
    }

    fn asl(&mut self, operand: &Operand) {
        let mut value = if let &Operand::Implied = operand {
            // Implied ASL uses the A register
            self.registers.A
        } else {
            self.unwrap_immediate(&operand)
        };

        // Test the seventh bit - if its set, shift it
        // into the carry flag
        self.flags.carry = (value & 0x80) == 0x80;

        // Shift the value left
        value = value << 0x01;
        self.flags.sign = value & 0x80 == 0x80;
        self.flags.zero = value as u8 & 0xFF == 0;

        if let &Operand::Implied = operand {
            self.registers.A = value;
        } else {
            let addr = self.unwrap_address(&operand);
            self.write_byte(addr, value);
        }
    }

    fn bcc(&mut self, operand: &Operand) {
        // Branch if the carry flag is not set
        if !self.flags.carry {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn bcs(&mut self, operand: &Operand) {
        // Branch if the carry flag is set
        if self.flags.carry {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn beq(&mut self, operand: &Operand) {
        // Branch if the zero flag is set
        if self.flags.zero {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn bit(&mut self, operand: &Operand) {
        let a = self.registers.A;
        let value = self.unwrap_immediate(&operand);
        let result = value & a;

        self.flags.zero = result == 0x00;
        self.flags.sign = value & 0x80 == 0x80;
        self.flags.overflow = value & 0x40 == 0x40;
    }

    fn bmi(&mut self, operand: &Operand) {
        // Branch if the sign flag is set
        if self.flags.sign {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn bne(&mut self, operand: &Operand) {
        // Branch if the zero flag is not set
        if !self.flags.zero {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn bpl(&mut self, operand: &Operand) {
        // Branch if the sign flag is not set
        if !self.flags.sign {
            let offset = self.unwrap_immediate(&operand);
            self.relative_jump(offset);
        }
    }

    fn brk(&mut self) {
        // Store the return address
        let mut mem = &mut self.memory;
        self.stack.push_u16(&mut mem[STACK_START..STACK_END], self.registers.PC + 0x01);
    }

    fn lda(&mut self, operand: &Operand) {
        let value = self.unwrap_immediate(&operand);

        self.registers.A = value;
        self.flags.sign = value & 0x80 == 0x80;
        self.flags.zero = value & 0xFF == 0x00;
    }

    fn sta(&mut self, operand: &Operand) {
        let addr = self.unwrap_address(&operand);
        let value = self.registers.A;

        self.write_byte(addr, value);
    }

    fn relative_jump(&mut self, offset: u8) {
        // If the sign bit is there, negate the PC by the difference
        // between 256 and the offset
        if offset & 0x80 == 0x80 {
            self.registers.PC -= 0x100 - offset as u16;
        } else {
            self.registers.PC += offset as u16;
        }
    }

    fn set_decimal_flag(&mut self, value: bool) {
        self.flags.decimal = value;
    }

    /// Convenience wrapper for accessing a byte
    /// in memory
    fn read_byte(&self, addr: u16) -> u8 {
        self.memory.read_byte(addr)
    }

    /// Convenience wrapper for writing a byte
    /// to memory
    fn write_byte(&mut self, addr: u16, byte: u8) {
        self.memory.write_byte(addr, byte);
    }

    /// Convenience wrapper for accessing a word
    /// in memory
    fn read_u16(&self, addr: u16) -> u16 {
        self.memory.read_u16(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpu::cpu_error::CpuError;

    #[test]
    fn can_instantiate_cpu() {
        let cpu = Cpu::new();

        assert!(0 == 0);
    }

    #[test]
    fn can_load_code_segment_into_memory() {
        let fake_code = vec![0x0A, 0x0B, 0x0C, 0x0D];
        let mut cpu = Cpu::new();
        cpu.load(&fake_code[..], None);

        let memory_sum: u32 = cpu.memory.iter().map(|n| *n as u32).sum();
        assert_eq!(46, memory_sum);
    }

    #[test]
    fn can_load_code_segment_at_default_address() {
        let fake_code = vec![0x0A, 0x0B, 0x0C, 0x0D];
        let mut cpu = Cpu::new();
        cpu.load(&fake_code[..], None);

        assert_eq!(0x0D, cpu.memory.read_byte(0xC003));
        assert_eq!(0x0C, cpu.memory.read_byte(0xC002));
        assert_eq!(0x0B, cpu.memory.read_byte(0xC001));
        assert_eq!(0x0A, cpu.memory.read_byte(0xC000));
    }

    #[test]
    fn can_load_code_segment_at_specific_address() {
        let fake_code = vec![0x0A, 0x0B, 0x0C, 0x0D];
        let mut cpu = Cpu::new();
        cpu.load(&fake_code[..], 0xF000);

        assert_eq!(0x0D, cpu.memory.read_byte(0xF003));
        assert_eq!(0x0C, cpu.memory.read_byte(0xF002));
        assert_eq!(0x0B, cpu.memory.read_byte(0xF001));
        assert_eq!(0x0A, cpu.memory.read_byte(0xF000));
    }

    #[test]
    fn errors_when_code_segment_extends_past_memory_bounds() {
        let fake_code = vec![0x0A, 0x0B, 0x0C, 0x0D];
        let mut cpu = Cpu::new();
        let load_result = cpu.load(&fake_code[..], 0xFFFD);

        assert_eq!(Err(CpuError::code_segment_out_of_range(0xFFFD)),
                   load_result);
    }

    #[test]
    fn errors_on_unknown_opcode() {
        let fake_code = vec![0xC3];
        let mut cpu = Cpu::new();
        cpu.load(&fake_code[..], None);
        let step_result: CpuStepResult = cpu.step();

        assert_eq!(Err(CpuError::unknown_opcode(0xC000, 0xC3)), step_result);// This is the unofficial DCP (d,X) opcode
    }

    #[test]
    fn can_get_operand_from_opcode() {
        let fake_code = vec![0xC3];
        let mut cpu = Cpu::new();
        cpu.load(&fake_code[..], None);
        let step_result: CpuStepResult = cpu.step();
    }

    #[test]
    fn adc_can_set_decimal_flag() {
        let code = vec![0xF8];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step();

        assert_eq!(true, cpu.flags.decimal);
    }

    #[test]
    fn adc_can_disable_decimal_flag() {
        let code = vec![0xD8];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step();

        assert_eq!(false, cpu.flags.decimal);
    }

    #[test]
    fn adc_can_add_basic_numbers() {
        let code = vec![0xA9, 0x05, 0x69, 0x03];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(8, cpu.registers.A);
    }

    #[test]
    fn adc_can_add_basic_numbers_set_carry_and_wrap_around() {
        let code = vec![0xA9, 0xFD, 0x69, 0x05];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(2, cpu.registers.A);
        assert_eq!(true, cpu.flags.carry);
    }

    #[test]
    fn adc_can_add_numbers_in_binary_coded_decimal() {
        let code = vec![0xF8, 0xA9, 0x05, 0x69, 0x05];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(3);

        assert_eq!(true, cpu.flags.decimal);
        assert_eq!(0x10, cpu.registers.A);
    }

    #[test]
    fn adc_can_add_numbers_in_binary_coded_decimal_and_set_carry() {
        let code = vec![0xF8, 0xA9, 0x95, 0x69, 0x10];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(3);

        assert_eq!(true, cpu.flags.carry);
        assert_eq!(true, cpu.flags.decimal);
        assert_eq!(0x05, cpu.registers.A);
    }

    #[test]
    fn sta_can_store_bytes_in_memory() {
        let code = vec![0xA9, 0x20, 0x8D, 0x00, 0x20];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(0x20, cpu.registers.A);
        assert_eq!(0x20, cpu.memory[0x2000]);
    }

    #[test]
    fn and_can_apply_logical_and_operation() {
        // Load 255 into A and mask it against 0x0F
        let code = vec![0xA9, 0xFF, 0x29, 0x0F];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(0x0F, cpu.registers.A);
        assert_eq!(false, cpu.flags.sign);
    }

    #[test]
    fn and_can_apply_logical_and_operation_and_set_sign_flag() {
        // Load 2 into the A register and shift it left
        let code = vec![0xA9, 0x02, 0x0A];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(0x04, cpu.registers.A);
        assert_eq!(false, cpu.flags.sign);
    }

    #[test]
    fn asl_can_shift_bits_left() {
        let code = vec![0xA9, 0x02, 0x0A];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(0x04, cpu.registers.A);
        assert_eq!(false, cpu.flags.sign);
    }

    #[test]
    fn asl_shifts_last_bit_into_carry() {
        let code = vec![0xA9, 0x80, 0x0A];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(2);

        assert_eq!(0x00, cpu.registers.A);
        assert_eq!(true, cpu.flags.carry);
    }

    #[test]
    fn bcc_can_jump_forward() {
        let code = vec![0xA9, 0xFE, 0x69, 0x01, 0x90, 0x03, 0xA9, 0x00];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(3);

        assert_eq!(0xFF, cpu.registers.A);
        assert_eq!(false, cpu.flags.carry);
        assert_eq!(0xC009, cpu.registers.PC);
    }

    #[test]
    fn bcc_can_jump_backward() {
        let code = vec![0xA9, 0xF0, 0x69, 0x01, 0x90, 0xFC];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(50);

        assert_eq!(0x00, cpu.registers.A);
    }

    #[test]
    fn bcs_can_jump_forward() {
        let code = vec![0xA9, 0xFF, 0x69, 0x01, 0xB0, 0x03, 0xA9, 0xAA];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0x00, cpu.registers.A);
        assert_eq!(true, cpu.flags.carry);
    }

    #[test]
    fn beq_can_jump_forward() {
        let code = vec![0xA9, 0xF0, 0x69, 0x10, 0xF0, 0x03, 0xA9, 0xAA];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0x00, cpu.registers.A);
    }

    #[test]
    fn bit_can_set_flags_and_preserve_registers() {
        let code = vec![0xA9, 0xF0, 0x24, 0x00];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(true, cpu.flags.zero);
        assert_eq!(0xF0, cpu.registers.A);  // Preserves A
    }

    #[test]
    fn bit_can_set_overflow_flag() {
        let code = vec![0xA9, 0xF0, 0x85, 0x44, 0x24, 0x44];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(false, cpu.flags.zero);
        assert_eq!(true, cpu.flags.overflow);
        assert_eq!(true, cpu.flags.sign);
        assert_eq!(0xF0, cpu.registers.A);  // Preserves A
    }

    #[test]
    fn bmi_can_jump_forward() {
        let code = vec![0xA9, 0x7F, 0x69, 0x01, 0x30, 0x03, 0xA9, 0x00];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0x80, cpu.registers.A);
        assert_eq!(true, cpu.flags.sign);
    }

    #[test]
    fn bne_jumps_on_non_zero() {
        let code = vec![0xA9, 0xFE, 0x69, 0x01, 0xD0, 0x03, 0xA9, 0xAA];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0xFF, cpu.registers.A);
        assert_eq!(false, cpu.flags.zero);
    }

    #[test]
    fn bne_does_not_jump_on_zero() {
        let code = vec![0xA9, 0xFF, 0x69, 0x01, 0xD0, 0x03, 0xA9, 0xAA];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0xAA, cpu.registers.A);
    }

    #[test]
    fn bpl_does_not_jump_on_sign_set() {
        let code = vec![0xA9, 0xFE, 0x10, 0x03, 0xA9, 0xF3];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0xF3, cpu.registers.A);
        assert_eq!(true, cpu.flags.sign);
    }

    #[test]
    fn bpl_does_jump_on_sign_not_set() {
        let code = vec![0xA9, 0x0E, 0x10, 0x03, 0xA9, 0xF3];
        let mut cpu = Cpu::new();
        cpu.load(&code[..], None);

        cpu.step_n(10);

        assert_eq!(0x0E, cpu.registers.A);
        assert_eq!(false, cpu.flags.sign);
    }
}
